//! Selection follows the observation's own mark paths without repainting fills or labels.

use richview_chart::geometry::{
    Cmd, Fill, Item, Panel, Path, Rect, Rgba, Role, Shape, Stroke, Wedge,
};

/// How far the soft halo reaches past a mark's contour, in CSS pixels (FEA-CH-53).
pub(super) const HALO_REACH: f64 = 4.0;
/// Where the contour's surface separator ends outside a filled mark's edge.
const SEPARATOR: f64 = 1.75;

/// The halo's paint: the mark's own colour at a quarter of its strength.
pub(super) fn halo(color: Rgba, level: f64) -> Rgba {
    color.with_alpha(0.25 * color.alpha() * level.clamp(0.0, 1.0))
}

/// Draw the existing hover-dot treatment at a contour grid node, never its hit box.
pub(super) fn node(
    panel: &Panel,
    observation: usize,
    level: f64,
    color: Rgba,
    surface: Rgba,
) -> Option<Vec<Item>> {
    let center = panel.observations.get(observation)?.at;
    if level <= 0.0 {
        return Some(Vec::new());
    }
    let scale = 0.4 + 0.6 * level;
    Some(
        [
            ((8.0 + HALO_REACH) * scale, halo(color, level)),
            (
                8.0 * scale,
                surface.with_alpha(surface.alpha() * level.min(1.0)),
            ),
            (
                6.0 * scale,
                color.with_alpha(color.alpha() * level.min(1.0)),
            ),
        ]
        .into_iter()
        .map(|(radius, fill)| Item::Point {
            center,
            radius,
            shape: Shape::Circle,
            fill: Some(Fill::solid(fill)),
            stroke: None,
            role: Role::Annotation,
            series: None,
            obs: None,
        })
        .collect(),
    )
}

/// Fade a contour along owned marks; other families retain their existing feedback.
pub(super) fn items(
    kind: &str,
    panel: &Panel,
    observation: usize,
    wedge: Option<Wedge>,
    ink: Rgba,
    surface: Rgba,
    level: f64,
) -> Option<Vec<Item>> {
    if kind == "gauge" {
        let mark = gauge_band(panel, observation);
        return Some(if level <= 0.0 {
            Vec::new()
        } else {
            mark.into_iter()
                .map(|mark| outline(mark, Stroke::solid(ink.with_alpha(0.85 * level), 1.5), 0.0))
                .collect()
        });
    }
    if kind == "map" && !panel.items.iter().any(|item| matches!(item, Item::Point { role: Role::Mark, obs: Some(index), .. } if *index == observation)) {
        return None;
    }
    if kind == "bar3d" {
        let Some((path, color)) = bar_silhouette(panel, observation) else {
            return Some(Vec::new());
        };
        if level <= 0.0 {
            return Some(Vec::new());
        }
        // Bars nearer the viewer paint over this one: trace only the edges the reader can see,
        // and keep the halo off the bars in front.
        let front = front_faces(panel, observation);
        let rings: Vec<Vec<[f64; 2]>> = path.flatten(0.5).into_iter().map(open_ring).collect();
        let line = |stroke: Stroke| Item::Line {
            path: visible_edges(&rings, &front),
            stroke,
            role: Role::Annotation,
            series: None,
            obs: None,
        };
        let mut result = vec![Item::Area {
            path: halo_outside(&rings, &front),
            fill: Fill::solid(halo(color, level)),
            role: Role::Annotation,
            series: None,
            obs: None,
        }];
        for (width, color) in [
            (3.5, surface.with_alpha(level)),
            (1.5, ink.with_alpha(0.85 * level)),
        ] {
            result.push(line(Stroke::solid(color, width)));
        }
        return Some(result);
    }
    if matches!(kind, "network" | "parallel" | "flow") {
        let mark = panel.items.iter().find(|mark| matches!(mark, Item::Line { role: Role::Mark, obs: Some(index), .. } if *index == observation));
        if let Some(Item::Line { path, stroke, .. }) = mark {
            if level <= 0.0 {
                return Some(Vec::new());
            }
            let line = |stroke: Stroke| Item::Line {
                path: path.clone(),
                stroke,
                role: Role::Annotation,
                series: None,
                obs: None,
            };
            let mut glow = stroke.clone();
            glow.color = stroke.color.with_alpha(0.25 * level);
            glow.width += 1.0 + 2.0 * HALO_REACH;
            let mut selected = stroke.clone();
            selected.color = stroke.color.with_alpha(stroke.color.alpha() * level);
            selected.width += level;
            return Some(vec![line(glow), line(selected)]);
        }
    }
    if matches!(kind, "hexbin" | "waffle") {
        return None;
    }
    if kind == "venn" {
        let items = if level <= 0.0 {
            Vec::new()
        } else {
            panel
                .items
                .iter()
                .find_map(|mark| match mark {
                    Item::Area {
                        path,
                        role: Role::Mark,
                        obs: Some(index),
                        ..
                    } if *index == observation => Some(Item::Line {
                        path: path.clone(),
                        stroke: Stroke::solid(ink.with_alpha(level), 1.5),
                        role: Role::Annotation,
                        series: None,
                        obs: None,
                    }),
                    _ => None,
                })
                .into_iter()
                .collect()
        };
        return Some(items);
    }
    if !matches!(
        kind,
        "ohlc"
            | "histogram"
            | "treemap"
            | "sunburst"
            | "scatter"
            | "packed-bubble"
            | "rose"
            | "pie"
            | "bar"
            | "combo"
            | "dumbbell"
            | "lollipop"
            | "sankey"
            | "alluvial"
            | "chord"
            | "boxplot"
            | "bullet"
            | "candlestick"
            | "interval"
            | "pareto"
            | "waterfall"
            | "gantt"
            | "funnel"
            | "violin"
            | "dotplot"
            | "qq"
            | "strip"
            | "scatter3d"
            | "network"
            | "map"
            | "flow"
    ) {
        return None;
    }
    let mut items = Vec::new();
    if level <= 0.0 {
        return Some(items);
    }
    let mut marks: Vec<_> = panel
        .items
        .iter()
        .filter(|mark| match mark {
            Item::Rect { obs, role, .. }
            | Item::Line { obs, role, .. }
            | Item::Area { obs, role, .. }
            | Item::Point { obs, role, .. } => *role == Role::Mark && *obs == Some(observation),
            Item::Text(_) => false,
        })
        // Grouped observations can own many shapes; existing single-shape families keep
        // their established selection, including the three components of an OHLC glyph.
        .take(match kind {
            "bar" | "combo" | "dumbbell" | "waffle" | "lollipop" | "boxplot" | "candlestick"
            | "interval" | "pareto" | "dotplot" => usize::MAX,
            "ohlc" => 3,
            _ => 1,
        })
        .cloned()
        .collect();
    // The body is painted over the middle of a candlestick's wick. Keep its feedback
    // on the exposed ends too, so the overlay does not draw through the body's fill.
    if kind == "candlestick"
        && let Some(body) = marks.iter().find_map(|mark| match mark {
            Item::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
    {
        for mark in &mut marks {
            if let Item::Line { path, .. } = mark {
                *path = exposed_wick(path, body);
            }
        }
    }
    // A soft halo in the mark's own colour replaces the ink glow where a mark can carry one.
    // Each is one filled band, painted once: a translucent stroke would overlap itself at
    // every join of a curve.
    let halos: Vec<Option<Item>> = marks
        .iter()
        .map(|mark| halo_band(kind, mark, wedge, level))
        .collect();
    items.extend(halos.iter().flatten().cloned());
    // Paint each layer across the whole glyph before the next. A tick's surface separator
    // must not erase its connected wick or a contour already drawn along that wick.
    for (layer, (extra, color, point_gap)) in [
        (6.0, ink.with_alpha(0.12 * level), 0.5),
        (3.5, surface.with_alpha(level), 0.5),
        (1.5, ink.with_alpha(0.85 * level), 2.5),
    ]
    .into_iter()
    .enumerate()
    {
        for (mark, halo) in marks.iter().zip(&halos) {
            if layer == 0 && halo.is_some() {
                continue;
            }
            let base = match mark {
                Item::Line { stroke, .. } => stroke.width,
                _ => 0.0,
            };
            items.push(outline(mark, Stroke::solid(color, base + extra), point_gap));
        }
    }
    // Restore the original coloured line components together, with their original widths.
    for mark in marks {
        if let Item::Line { stroke, .. } = &mark {
            items.push(outline(
                &mark,
                Stroke {
                    color: stroke.color.with_alpha(stroke.color.alpha() * level),
                    ..stroke.clone()
                },
                0.0,
            ));
        }
    }
    Some(items)
}

/// Find the band at the needle without changing the gauge's larger pointer target.
fn gauge_band(panel: &Panel, observation: usize) -> Option<&Item> {
    let point = panel.observations.get(observation)?.at;
    let mut track = None;
    let mut best: Option<(&Item, bool, f64)> = None;
    for item in &panel.items {
        match item {
            Item::Area {
                role: Role::Band, ..
            } if track.is_none() => track = Some(item),
            Item::Area {
                path,
                role: Role::Mark,
                ..
            } if track.is_some() => {
                let (inside, distance) = path_distance(path, point);
                let replace = best.is_none_or(|(_, was_inside, old)| {
                    (inside && !was_inside) || (inside == was_inside && distance <= old + 0.1)
                });
                if replace {
                    best = Some((item, inside, distance));
                }
            }
            Item::Line {
                role: Role::Mark, ..
            } if track.is_some() => break,
            _ => {}
        }
    }
    best.map(|(item, _, _)| item).or(track)
}

/// Whether a point lies in a filled path and its distance to the path's boundary.
fn path_distance(path: &Path, point: [f64; 2]) -> (bool, f64) {
    let mut inside = false;
    let mut distance = f64::INFINITY;
    for ring in path.flatten(0.25) {
        for pair in ring.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let edge = [b[0] - a[0], b[1] - a[1]];
            let length = edge[0] * edge[0] + edge[1] * edge[1];
            let t = if length > 0.0 {
                (((point[0] - a[0]) * edge[0] + (point[1] - a[1]) * edge[1]) / length)
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            distance =
                distance.min((a[0] + t * edge[0] - point[0]).hypot(a[1] + t * edge[1] - point[1]));
            if (a[1] > point[1]) != (b[1] > point[1])
                && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
            {
                inside = !inside;
            }
        }
    }
    (inside, distance)
}

/// Keep only the two ends of an axis-aligned wick that are outside the candle body.
fn exposed_wick(path: &Path, body: Rect) -> Path {
    let [Cmd::M { p: a }, Cmd::L { p: b }] = path.cmds.as_slice() else {
        return path.clone();
    };
    let vertical = (a[0] - b[0]).abs() < 1e-9;
    let horizontal = (a[1] - b[1]).abs() < 1e-9;
    if !vertical && !horizontal {
        return path.clone();
    }
    let (axis, low_edge, high_edge) = if vertical {
        (1, body.y, body.bottom())
    } else {
        (0, body.x, body.right())
    };
    let low = a[axis].min(b[axis]);
    let high = a[axis].max(b[axis]);
    let point = |value| {
        let mut p = *a;
        p[axis] = value;
        p
    };
    let mut result = Path::new();
    if low < low_edge {
        result.move_to(point(low));
        result.line_to(point(high.min(low_edge)));
    }
    if high > high_edge {
        result.move_to(point(low.max(high_edge)));
        result.line_to(point(high));
    }
    result
}

/// The outside edges of the visible faces of one projected bar, with shared edges removed.
fn bar_silhouette(panel: &Panel, observation: usize) -> Option<(Path, Rgba)> {
    let mut edges: Vec<([f64; 2], [f64; 2])> = Vec::new();
    let mut color = None;
    for mark in &panel.items {
        let Item::Area {
            path,
            fill,
            role: Role::Mark,
            obs: Some(index),
            ..
        } = mark
        else {
            continue;
        };
        if *index != observation {
            continue;
        }
        color.get_or_insert(fill.color);
        for ring in path.flatten(0.5) {
            for pair in ring.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if let Some(i) = edges
                    .iter()
                    .position(|(x, y)| (*x == a && *y == b) || (*x == b && *y == a))
                {
                    edges.remove(i);
                } else {
                    edges.push((a, b));
                }
            }
        }
    }
    let mut path = Path::new();
    while let Some((start, mut at)) = edges.pop() {
        path.move_to(start);
        path.line_to(at);
        while at != start {
            let i = edges.iter().position(|(a, b)| *a == at || *b == at)?;
            let (a, b) = edges.remove(i);
            at = if a == at { b } else { a };
            path.line_to(at);
        }
        path.close();
    }
    Some((path, color?))
}

/// The faces of other bars painted after this one's, which hide whatever of it they cover.
fn front_faces(panel: &Panel, observation: usize) -> Vec<Vec<[f64; 2]>> {
    let own = |item: &Item| matches!(item, Item::Area { role: Role::Mark, obs: Some(index), .. } if *index == observation);
    let Some(last) = panel.items.iter().rposition(own) else {
        return Vec::new();
    };
    panel.items[last + 1..]
        .iter()
        .filter_map(|item| match item {
            Item::Area {
                path,
                role: Role::Mark,
                obs: Some(index),
                ..
            } if *index != observation => Some(path.flatten(0.5)),
            _ => None,
        })
        .flatten()
        .map(open_ring)
        .filter(|ring| ring.len() >= 3)
        .collect()
}

/// A flattened ring without its closing point.
fn open_ring(mut ring: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    if ring.len() > 1 && ring.first() == ring.last() {
        ring.pop();
    }
    ring
}

/// Twice the signed area of a ring: positive for one turning direction, negative for the other.
fn signed_area(ring: &[[f64; 2]]) -> f64 {
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

/// Which side of the edge from `a` to `b` a point lies on, scaled by the edge's length:
/// positive inside a ring of turning direction `turn`.
fn side(a: [f64; 2], b: [f64; 2], p: [f64; 2], turn: f64) -> f64 {
    turn * ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]))
}

/// The stretch of the segment from `a` to `b`, as fractions of it, that lies inside a convex
/// polygon (Cyrus-Beck).
fn inside_span(a: [f64; 2], b: [f64; 2], polygon: &[[f64; 2]]) -> Option<(f64, f64)> {
    let turn = signed_area(polygon).signum();
    if turn == 0.0 {
        return None;
    }
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for i in 0..polygon.len() {
        let (p, q) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        let (sa, sb) = (side(p, q, a, turn), side(p, q, b, turn));
        if sa <= 0.0 && sb <= 0.0 {
            return None;
        }
        if sa < 0.0 || sb < 0.0 {
            let t = sa / (sa - sb);
            if sa < 0.0 {
                t0 = t0.max(t)
            } else {
                t1 = t1.min(t)
            }
        }
    }
    (t1 - t0 > 1e-9).then_some((t0, t1))
}

/// The silhouette's edges, less what the faces in front hide, as open polylines; a fully
/// visible ring stays closed.
fn visible_edges(rings: &[Vec<[f64; 2]>], front: &[Vec<[f64; 2]>]) -> Path {
    let mut path = Path::new();
    for ring in rings {
        let n = ring.len();
        let mut pieces: Vec<([f64; 2], [f64; 2])> = Vec::new();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let mut shown = vec![(0.0_f64, 1.0_f64)];
            for face in front {
                let Some((h0, h1)) = inside_span(a, b, face) else {
                    continue;
                };
                shown = shown
                    .into_iter()
                    .flat_map(|(s0, s1)| [(s0, s1.min(h0)), (s0.max(h1), s1)])
                    .filter(|(s0, s1)| s1 - s0 > 1e-6)
                    .collect();
            }
            let at = |t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            pieces.extend(shown.into_iter().map(|(s0, s1)| (at(s0), at(s1))));
        }
        if pieces.len() == n && pieces.iter().zip(ring).all(|((p, _), v)| p == v) {
            path.move_to(ring[0]);
            for &v in &ring[1..] {
                path.line_to(v);
            }
            path.close();
            continue;
        }
        // Join pieces that meet into polylines, the last into the first when it runs on.
        let mut lines: Vec<Vec<[f64; 2]>> = Vec::new();
        for (p0, p1) in pieces {
            match lines.last_mut() {
                Some(line) if line.last() == Some(&p0) => line.push(p1),
                _ => lines.push(vec![p0, p1]),
            }
        }
        if lines.len() > 1
            && lines.last().and_then(|l| l.last()) == lines.first().and_then(|l| l.first())
        {
            let mut tail = lines.pop().unwrap_or_default();
            tail.extend(lines[0].drain(1..));
            lines[0] = tail;
        }
        for line in lines {
            path.move_to(line[0]);
            for &v in &line[1..] {
                path.line_to(v);
            }
        }
    }
    path
}

/// The part of a convex polygon on the inner side of the edge from `p` to `q` (`keep` > 0) or
/// on its outer side (`keep` < 0), for a clipping polygon of turning direction `turn`.
fn clip_half(
    polygon: &[[f64; 2]],
    p: [f64; 2],
    q: [f64; 2],
    turn: f64,
    keep: f64,
) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        let (sa, sb) = (keep * side(p, q, a, turn), keep * side(p, q, b, turn));
        if sa >= 0.0 {
            out.push(a);
        }
        if (sa >= 0.0) != (sb >= 0.0) {
            let t = sa / (sa - sb);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

/// A convex polygon less a convex hole, as convex pieces.
fn subtract(polygon: Vec<[f64; 2]>, hole: &[[f64; 2]]) -> Vec<Vec<[f64; 2]>> {
    let turn = signed_area(hole).signum();
    if turn == 0.0 {
        return vec![polygon];
    }
    let mut pieces = Vec::new();
    let mut rest = polygon;
    for i in 0..hole.len() {
        let (p, q) = (hole[i], hole[(i + 1) % hole.len()]);
        let outside = clip_half(&rest, p, q, turn, -1.0);
        if signed_area(&outside).abs() > 1e-6 {
            pieces.push(outside);
        }
        rest = clip_half(&rest, p, q, turn, 1.0);
        if signed_area(&rest).abs() <= 1e-6 {
            return pieces;
        }
    }
    pieces
}

/// The halo band outside a convex silhouette, between `SEPARATOR` and `SEPARATOR + HALO_REACH`
/// from it, less the faces in front.
fn halo_outside(rings: &[Vec<[f64; 2]>], front: &[Vec<[f64; 2]>]) -> Path {
    let mut quads: Vec<Vec<[f64; 2]>> = Vec::new();
    for ring in rings {
        let n = ring.len();
        let turn = signed_area(ring).signum();
        if n < 3 || turn == 0.0 {
            continue;
        }
        // The outward normal of the edge leaving vertex i.
        let normal = |i: usize| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let d = [b[0] - a[0], b[1] - a[1]];
            let len = d[0].hypot(d[1]).max(1e-9);
            [turn * d[1] / len, -turn * d[0] / len]
        };
        let off = |p: [f64; 2], m: [f64; 2], r: f64| [p[0] + m[0] * r, p[1] + m[1] * r];
        let (inner, outer) = (SEPARATOR, SEPARATOR + HALO_REACH);
        for i in 0..n {
            let (a, b, m) = (ring[i], ring[(i + 1) % n], normal(i));
            quads.push(vec![
                off(a, m, inner),
                off(b, m, inner),
                off(b, m, outer),
                off(a, m, outer),
            ]);
            // The corner at b, between this edge's normal and the next one's.
            let next = normal((i + 1) % n);
            if (m[0] - next[0]).hypot(m[1] - next[1]) > 1e-6 {
                quads.push(vec![
                    off(b, m, inner),
                    off(b, m, outer),
                    off(b, next, outer),
                    off(b, next, inner),
                ]);
            }
        }
    }
    let mut pieces = quads;
    for face in front {
        pieces = pieces
            .into_iter()
            .flat_map(|piece| subtract(piece, face))
            .collect();
    }
    let mut path = Path::new();
    for piece in pieces.iter().filter(|piece| piece.len() >= 3) {
        path.move_to(piece[0]);
        for &v in &piece[1..] {
            path.line_to(v);
        }
        path.close();
    }
    path
}

/// Trace tiled observations in ink, without a halo over their neighbours.
pub(super) fn tiles(
    kind: &str,
    panel: &Panel,
    observation: usize,
    ink: Rgba,
    level: f64,
) -> Option<Vec<Item>> {
    if !matches!(
        kind,
        "heatmap"
            | "calendar"
            | "bin2d"
            | "hexbin"
            | "pivot"
            | "icicle"
            | "waffle"
            | "map"
            | "surface"
    ) {
        return None;
    }
    if level <= 0.0 {
        return Some(Vec::new());
    }
    Some(
        panel
            .items
            .iter()
            .filter(|item| match item {
                Item::Rect {
                    role: Role::Mark,
                    obs: Some(index),
                    ..
                }
                | Item::Area {
                    role: Role::Mark,
                    obs: Some(index),
                    ..
                } => *index == observation,
                _ => false,
            })
            .map(|mark| outline(mark, Stroke::solid(ink.with_alpha(0.85 * level), 1.5), 0.0))
            .collect(),
    )
}

/// Trace a mark’s shape, keeping point contours beyond the original marker.
fn outline(mark: &Item, mut stroke: Stroke, point_gap: f64) -> Item {
    match mark {
        Item::Rect { rect, radius, .. } => Item::Rect {
            rect: *rect,
            fill: Fill::solid(Rgba([0.0; 4])),
            stroke: Some(stroke),
            radius: *radius,
            role: Role::Annotation,
            series: None,
            obs: None,
        },
        Item::Line {
            path,
            stroke: original,
            ..
        } => {
            stroke.cap = original.cap;
            stroke.dash = original.dash.clone();
            Item::Line {
                path: path.clone(),
                stroke,
                role: Role::Annotation,
                series: None,
                obs: None,
            }
        }
        Item::Area { path, .. } => Item::Line {
            path: path.clone(),
            stroke,
            role: Role::Annotation,
            series: None,
            obs: None,
        },
        Item::Point {
            center,
            radius,
            shape,
            stroke: original,
            ..
        } => {
            let original_half = original.as_ref().map_or(0.0, |s| s.width / 2.0);
            // Leave the original mark and its stroke untouched. The final ink line starts
            // beyond the surface separator; the glow and separator start just outside it.
            if matches!(shape, Shape::Plus | Shape::Cross) {
                return cross_ring(*center, *radius, *shape, original_half, point_gap, stroke);
            }
            let inradius = match shape {
                Shape::Circle => 1.0,
                Shape::Square => 0.85,
                Shape::Triangle => 0.48,
                Shape::Diamond => std::f64::consts::FRAC_1_SQRT_2,
                Shape::Plus | Shape::Cross => unreachable!(),
            };
            Item::Point {
                center: *center,
                radius: radius + (original_half + point_gap + stroke.width / 2.0) / inradius,
                shape: *shape,
                fill: None,
                stroke: Some(stroke),
                role: Role::Annotation,
                series: None,
                obs: None,
            }
        }
        Item::Text(_) => unreachable!("only owned marks have contours"),
    }
}

/// Draw the outside of a stroked plus or cross without painting either original bar.
fn cross_ring(
    center: [f64; 2],
    radius: f64,
    shape: Shape,
    original_half: f64,
    gap: f64,
    stroke: Stroke,
) -> Item {
    let rotated = shape == Shape::Cross;
    let reach = if rotated {
        radius * 0.75 * std::f64::consts::SQRT_2
    } else {
        radius
    };
    let mut path = Path::new();
    for offset in [gap + stroke.width, gap] {
        let half = original_half + offset;
        let end = reach + offset;
        let points = [
            [-half, -end],
            [half, -end],
            [half, -half],
            [end, -half],
            [end, half],
            [half, half],
            [half, end],
            [-half, end],
            [-half, half],
            [-end, half],
            [-end, -half],
            [-half, -half],
        ];
        for (i, [x, y]) in points.into_iter().enumerate() {
            let [x, y] = if rotated {
                [
                    (x - y) * std::f64::consts::FRAC_1_SQRT_2,
                    (x + y) * std::f64::consts::FRAC_1_SQRT_2,
                ]
            } else {
                [x, y]
            };
            let point = [center[0] + x, center[1] + y];
            if i == 0 {
                path.move_to(point);
            } else {
                path.line_to(point);
            }
        }
        path.close();
    }
    Item::Area {
        path,
        fill: Fill::solid(stroke.color),
        role: Role::Annotation,
        series: None,
        obs: None,
    }
}

/// The halo of one mark as a band outside its contour, or none for a mark that keeps the
/// ink glow: lines, and filled marks whose neighbours touch them.
fn halo_band(kind: &str, mark: &Item, wedge: Option<Wedge>, level: f64) -> Option<Item> {
    let (path, color) = match mark {
        Item::Point {
            center,
            radius,
            shape,
            fill,
            stroke,
            ..
        } => {
            let color = fill.map(|f| f.color).or(stroke.as_ref().map(|s| s.color))?;
            let half = stroke.as_ref().map_or(0.0, |s| s.width / 2.0);
            // The contour's ink line ends 4 px outside the marker; a cross reaches its corners.
            let reach = match shape {
                Shape::Circle => *radius,
                Shape::Square => radius * 0.85 * std::f64::consts::SQRT_2,
                Shape::Triangle | Shape::Diamond | Shape::Plus | Shape::Cross => *radius,
            };
            let inner = reach + half + 4.0;
            let mut path = Path::new();
            ring(&mut path, *center, inner + HALO_REACH);
            ring(&mut path, *center, inner);
            (path, color)
        }
        Item::Area { fill, .. } if matches!(kind, "pie" | "rose") => {
            let w = wedge?;
            if w.end - w.start <= 0.0 {
                return None;
            }
            let (inner, outer) = (w.radius + SEPARATOR, w.radius + SEPARATOR + HALO_REACH);
            let steps = (((w.end - w.start) * outer / 3.0).ceil() as usize).clamp(2, 240);
            let at = |r: f64, k: usize| {
                let a = w.start + (w.end - w.start) * k as f64 / steps as f64
                    - std::f64::consts::FRAC_PI_2;
                [w.center[0] + r * a.cos(), w.center[1] + r * a.sin()]
            };
            let mut path = Path::new();
            path.move_to(at(inner, 0));
            for k in 1..=steps {
                path.line_to(at(inner, k));
            }
            for k in (0..=steps).rev() {
                path.line_to(at(outer, k));
            }
            path.close();
            (path, fill.color)
        }
        Item::Rect {
            rect, fill, stroke, ..
        } if matches!(
            kind,
            "bar"
                | "combo"
                | "boxplot"
                | "bullet"
                | "candlestick"
                | "pareto"
                | "waterfall"
                | "gantt"
                | "funnel"
                | "flow"
        ) =>
        {
            let mut path = Path::new();
            rounded(&mut path, rect.inflate(SEPARATOR), SEPARATOR + HALO_REACH);
            rounded(&mut path, rect.inflate(SEPARATOR), SEPARATOR);
            (path, stroke.as_ref().map_or(fill.color, |s| s.color))
        }
        Item::Line { path, stroke, .. }
            if matches!(kind, "boxplot" | "candlestick" | "interval") =>
        {
            let mut halo_stroke = stroke.clone();
            halo_stroke.color = halo(stroke.color, level);
            halo_stroke.width += 2.0 * (SEPARATOR + HALO_REACH);
            return Some(Item::Line {
                path: path.clone(),
                stroke: halo_stroke,
                role: Role::Annotation,
                series: None,
                obs: None,
            });
        }
        Item::Area { path, fill, .. } if kind == "violin" => {
            let mut band = Path::new();
            for points in path.flatten(0.5) {
                offset_polygon(&mut band, &points, SEPARATOR + HALO_REACH);
                offset_polygon(&mut band, &points, SEPARATOR);
            }
            (band, fill.color)
        }
        _ => return None,
    };
    Some(Item::Area {
        path,
        fill: Fill::solid(halo(color, level)),
        role: Role::Annotation,
        series: None,
        obs: None,
    })
}

/// Trace an offset polygon, capping mitres at sharp turns in a curved silhouette.
fn offset_polygon(path: &mut Path, points: &[[f64; 2]], distance: f64) {
    let n = if points.first() == points.last() {
        points.len().saturating_sub(1)
    } else {
        points.len()
    };
    if n < 3 {
        return;
    }
    let area: f64 = (0..n)
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % n];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    if area.abs() < 1e-9 {
        return;
    }
    for i in 0..n {
        let p = points[i];
        let previous = points[(i + n - 1) % n];
        let next = points[(i + 1) % n];
        let normal = |a: [f64; 2], b: [f64; 2]| {
            let d = [b[0] - a[0], b[1] - a[1]];
            let len = d[0].hypot(d[1]).max(1e-9);
            [area.signum() * d[1] / len, -area.signum() * d[0] / len]
        };
        let a = normal(previous, p);
        let b = normal(p, next);
        let scale = (distance / (1.0 + a[0] * b[0] + a[1] * b[1]).max(0.25)).min(distance * 4.0);
        let q = [p[0] + (a[0] + b[0]) * scale, p[1] + (a[1] + b[1]) * scale];
        if i == 0 {
            path.move_to(q);
        } else {
            path.line_to(q);
        }
    }
    path.close();
}

/// A circle as one closed polygon.
fn ring(path: &mut Path, center: [f64; 2], radius: f64) {
    let steps = ((radius * 2.0).ceil() as usize).clamp(24, 96);
    for k in 0..steps {
        let a = std::f64::consts::TAU * k as f64 / steps as f64;
        let point = [center[0] + radius * a.cos(), center[1] + radius * a.sin()];
        if k == 0 {
            path.move_to(point);
        } else {
            path.line_to(point);
        }
    }
    path.close();
}

/// `rect` grown by `reach - corner` with corners rounded to `reach`: the outline `reach`
/// away from a rectangle whose own corners are `corner` inside it.
fn rounded(path: &mut Path, rect: Rect, reach: f64) {
    let grow = reach - SEPARATOR;
    let r = rect.inflate(grow);
    let radius = reach.min(r.w / 2.0).min(r.h / 2.0);
    let corners = [
        ([r.right() - radius, r.y + radius], -90.0_f64),
        ([r.right() - radius, r.bottom() - radius], 0.0),
        ([r.x + radius, r.bottom() - radius], 90.0),
        ([r.x + radius, r.y + radius], 180.0),
    ];
    let mut first = true;
    for (c, from) in corners {
        for k in 0..=6 {
            let a = (from + 90.0 * k as f64 / 6.0).to_radians();
            let point = [c[0] + radius * a.cos(), c[1] + radius * a.sin()];
            if first {
                path.move_to(point);
                first = false;
            } else {
                path.line_to(point);
            }
        }
    }
    path.close();
}

#[cfg(test)]
mod tests {
    use super::*;
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{ChartTheme, Options, layout};

    #[test]
    fn separated_marks_follow_their_observation_geometry() {
        for (kind, fixture) in [
            (
                "boxplot",
                r#"{"spec":{"type":"boxplot","title":"T","encoding":{"x":"k","y":"v"}},"rows":[{"k":"a","v":1},{"k":"a","v":2},{"k":"a","v":3},{"k":"a","v":4},{"k":"a","v":50},{"k":"b","v":2},{"k":"b","v":3}]}"#,
            ),
            (
                "bullet",
                include_str!("../../../../../richview-chart/tests/fixtures/types/bullet.json"),
            ),
            (
                "candlestick",
                include_str!("../../../../../richview-chart/tests/fixtures/types/candlestick.json"),
            ),
            (
                "interval",
                r#"{"spec":{"type":"interval","title":"T","encoding":{"x":"k","y":"v","lower":"lo","upper":"hi"}},"rows":[{"k":"a","v":2,"lo":1,"hi":3},{"k":"b","v":4,"lo":3,"hi":5}]}"#,
            ),
            (
                "pareto",
                include_str!("../../../../../richview-chart/tests/fixtures/types/pareto.json"),
            ),
            (
                "waterfall",
                include_str!("../../../../../richview-chart/tests/fixtures/types/waterfall.json"),
            ),
            (
                "gantt",
                include_str!("../../../../../richview-chart/tests/fixtures/types/gantt.json"),
            ),
            (
                "funnel",
                include_str!("../../../../../richview-chart/tests/fixtures/types/funnel.json"),
            ),
            (
                "violin",
                include_str!("../../../../../richview-chart/tests/fixtures/types/violin.json"),
            ),
            (
                "dotplot",
                include_str!("../../../../../richview-chart/tests/fixtures/types/dotplot.json"),
            ),
            (
                "qq",
                include_str!("../../../../../richview-chart/tests/fixtures/types/qq.json"),
            ),
            (
                "strip",
                include_str!("../../../../../richview-chart/tests/fixtures/types/strip.json"),
            ),
            (
                "scatter3d",
                include_str!("../../../../../richview-chart/tests/fixtures/types/scatter3d.json"),
            ),
        ] {
            let input: serde_json::Value = serde_json::from_str(fixture).unwrap();
            let geometry = layout(
                &ChartBlock::from_value(&input["spec"]).unwrap(),
                input["rows"].as_array().unwrap(),
                &ChartTheme::default(),
                &Options::default(),
            );
            let panel = &geometry.panels[0];
            for (index, observation) in panel.observations.iter().enumerate() {
                let marks: Vec<_> = panel.items.iter().filter(|item| matches!(item, Item::Rect { role: Role::Mark, obs: Some(k), .. } | Item::Line { role: Role::Mark, obs: Some(k), .. } | Item::Area { role: Role::Mark, obs: Some(k), .. } | Item::Point { role: Role::Mark, obs: Some(k), .. } if *k == index)).collect();
                assert!(
                    !marks.is_empty(),
                    "{kind} observation {index} has no owned mark"
                );
                let contours = items(
                    kind,
                    panel,
                    index,
                    observation.wedge,
                    Rgba([0.0, 0.0, 0.0, 1.0]),
                    geometry.background,
                    1.0,
                )
                .unwrap_or_else(|| panic!("{kind} has no contour"));
                let body = marks.iter().find_map(|mark| match mark {
                    Item::Rect { rect, .. } if kind == "candlestick" => Some(*rect),
                    _ => None,
                });
                let follows = |mark: &Item, item: &Item| match (mark, item) {
                    (Item::Rect { rect: a, .. }, Item::Rect { rect: b, .. }) => a == b,
                    (Item::Line { path: a, .. }, Item::Line { path: b, .. }) => {
                        a == b || body.is_some_and(|rect| exposed_wick(a, rect) == *b)
                    }
                    (Item::Area { path: a, .. }, Item::Line { path: b, .. }) => a == b,
                    (Item::Point { center: a, .. }, Item::Point { center: b, .. }) => a == b,
                    _ => false,
                };
                for mark in &marks {
                    assert!(
                        contours.iter().any(|item| follows(mark, item)),
                        "{kind} observation {index} misses a mark: {mark:?}"
                    );
                }
                for item in &contours {
                    let owned = if matches!(item, Item::Area { .. }) {
                        item.bounds().is_some_and(|r| {
                            marks.iter().any(|mark| {
                                mark.bounds().is_some_and(|b| {
                                    let b = b.inflate(25.0);
                                    r.x >= b.x
                                        && r.y >= b.y
                                        && r.right() <= b.right()
                                        && r.bottom() <= b.bottom()
                                })
                            })
                        })
                    } else {
                        marks.iter().any(|mark| follows(mark, item))
                    };
                    assert!(
                        owned,
                        "{kind} observation {index} contours an unrelated shape: {item:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn candlestick_wick_feedback_stays_outside_the_body() {
        let body = Rect::new(10.0, 20.0, 8.0, 12.0);
        for (wick, expected) in [
            (
                Path::polyline(&[[14.0, 10.0], [14.0, 40.0]]),
                vec![
                    vec![[14.0, 10.0], [14.0, 20.0]],
                    vec![[14.0, 32.0], [14.0, 40.0]],
                ],
            ),
            (
                Path::polyline(&[[0.0, 26.0], [25.0, 26.0]]),
                vec![
                    vec![[0.0, 26.0], [10.0, 26.0]],
                    vec![[18.0, 26.0], [25.0, 26.0]],
                ],
            ),
        ] {
            assert_eq!(exposed_wick(&wick, body).flatten(0.5), expected);
        }
    }

    #[test]
    fn tiled_observations_outline_only_their_own_tiles_without_halos() {
        for (kind, fixture) in [
            (
                "heatmap",
                r#"{"spec":{"type":"heatmap","title":"T","encoding":{"x":"hour","y":"day","color":"value"}},"rows":[{"hour":"9","day":"Mon","value":2},{"hour":"12","day":"Mon","value":4},{"hour":"9","day":"Tue","value":3}]}"#,
            ),
            (
                "calendar",
                include_str!("../../../../../richview-chart/tests/fixtures/types/calendar.json"),
            ),
            (
                "bin2d",
                include_str!("../../../../../richview-chart/tests/fixtures/types/bin2d.json"),
            ),
            (
                "hexbin",
                include_str!("../../../../../richview-chart/tests/fixtures/types/hexbin.json"),
            ),
            (
                "pivot",
                include_str!("../../../../../richview-chart/tests/fixtures/types/pivot.json"),
            ),
            (
                "icicle",
                include_str!("../../../../../richview-chart/tests/fixtures/types/icicle.json"),
            ),
            (
                "waffle",
                include_str!("../../../../../richview-chart/tests/fixtures/types/waffle.json"),
            ),
        ] {
            let input: serde_json::Value = serde_json::from_str(fixture).unwrap();
            let geometry = layout(
                &ChartBlock::from_value(&input["spec"]).unwrap(),
                input["rows"].as_array().unwrap(),
                &ChartTheme::default(),
                &Options::default(),
            );
            for panel in &geometry.panels {
                for index in 0..panel.observations.len() {
                    let marks: Vec<_> = panel.items.iter().filter(|item| matches!(item,
                        Item::Rect { role: Role::Mark, obs: Some(k), .. } | Item::Area { role: Role::Mark, obs: Some(k), .. } if *k == index)).collect();
                    assert!(
                        !marks.is_empty(),
                        "{kind} observation {index} has no owned tile"
                    );
                    let ink = Rgba([0.2, 0.3, 0.4, 1.0]);
                    let outlines = tiles(kind, panel, index, ink, 1.0).expect("a tiled family");
                    assert_eq!(
                        outlines.len(),
                        marks.len(),
                        "{kind} observation {index} has extra feedback or misses a tile"
                    );
                    for (mark, outline) in marks.into_iter().zip(outlines.iter()) {
                        match (mark, outline) {
                            (
                                Item::Rect { rect: a, .. },
                                Item::Rect {
                                    rect: b,
                                    stroke: Some(stroke),
                                    ..
                                },
                            ) => {
                                assert_eq!(a, b);
                                assert_eq!(*stroke, Stroke::solid(ink.with_alpha(0.85), 1.5));
                            }
                            (
                                Item::Area { path: a, .. },
                                Item::Line {
                                    path: b, stroke, ..
                                },
                            ) => {
                                assert_eq!(a, b);
                                assert_eq!(*stroke, Stroke::solid(ink.with_alpha(0.85), 1.5));
                            }
                            _ => panic!("{kind} observation {index} has the wrong contour"),
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn parallel_network_and_flow_lines_thicken_without_a_stray_point() {
        for (kind, fixture) in [
            (
                "parallel",
                include_str!("../../../../../richview-chart/tests/fixtures/types/parallel.json"),
            ),
            (
                "network",
                include_str!("../../../../../richview-chart/tests/fixtures/types/network.json"),
            ),
            (
                "flow",
                include_str!("../../../../../richview-chart/tests/fixtures/types/flow.json"),
            ),
        ] {
            let input: serde_json::Value = serde_json::from_str(fixture).unwrap();
            let geometry = layout(
                &ChartBlock::from_value(&input["spec"]).unwrap(),
                input["rows"].as_array().unwrap(),
                &ChartTheme::default(),
                &Options::default(),
            );
            let panel = &geometry.panels[0];
            let lines: Vec<_> = panel
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Line {
                        path,
                        stroke,
                        role: Role::Mark,
                        obs: Some(index),
                        ..
                    } => Some((*index, path, stroke)),
                    _ => None,
                })
                .collect();
            assert!(!lines.is_empty(), "{kind} has selectable lines");
            for (index, path, stroke) in lines {
                let items = items(
                    kind,
                    panel,
                    index,
                    None,
                    Rgba([0.0, 0.0, 0.0, 1.0]),
                    geometry.background,
                    1.0,
                )
                .unwrap();
                assert!(
                    matches!(items.as_slice(), [Item::Line { path: halo_path, .. }, Item::Line { path: selected_path, stroke: selected, .. }] if halo_path == path && selected_path == path && (selected.width - stroke.width - 1.0).abs() < 1e-9),
                    "{kind} must thicken the selected path without a point: {items:?}"
                );
            }
        }
    }

    #[test]
    fn bar3d_contour_traces_only_what_the_bars_in_front_leave_visible() {
        let input: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/bar3d.json"
        ))
        .unwrap();
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let panel = &geometry.panels[0];
        // Strictly inside a front face, a quarter pixel in from each of its edges.
        let hidden = |p: [f64; 2], face: &[[f64; 2]]| {
            let turn = signed_area(face).signum();
            (0..face.len()).all(|i| {
                let (a, b) = (face[i], face[(i + 1) % face.len()]);
                side(a, b, p, turn) / (b[0] - a[0]).hypot(b[1] - a[1]) > 0.25
            })
        };
        let length = |rings: Vec<Vec<[f64; 2]>>| -> f64 {
            rings
                .iter()
                .flat_map(|ring| ring.windows(2))
                .map(|pair| (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]))
                .sum()
        };
        let (mut partly, mut whole) = (0, 0);
        for index in 0..panel.observations.len() {
            let front = front_faces(panel, index);
            let contour = items(
                "bar3d",
                panel,
                index,
                None,
                Rgba([0.0, 0.0, 0.0, 1.0]),
                geometry.background,
                1.0,
            )
            .unwrap();
            let [
                Item::Area { path: band, .. },
                Item::Line {
                    path: separator, ..
                },
                Item::Line { path: ink, .. },
            ] = contour.as_slice()
            else {
                panic!("bar {index}: a halo, a separator and an ink line");
            };
            assert_eq!(separator.flatten(0.5), ink.flatten(0.5));
            for ring in ink.flatten(0.5) {
                for pair in ring.windows(2) {
                    for step in 0..=20 {
                        let t = f64::from(step) / 20.0;
                        let p = [
                            pair[0][0] + (pair[1][0] - pair[0][0]) * t,
                            pair[0][1] + (pair[1][1] - pair[0][1]) * t,
                        ];
                        assert!(
                            !front.iter().any(|face| hidden(p, face)),
                            "bar {index}: its contour runs over a bar in front at {p:?}"
                        );
                    }
                }
            }
            for piece in band.flatten(0.5) {
                let piece = open_ring(piece);
                let n = piece.len() as f64;
                let centre = [
                    piece.iter().map(|p| p[0]).sum::<f64>() / n,
                    piece.iter().map(|p| p[1]).sum::<f64>() / n,
                ];
                assert!(
                    !front.iter().any(|face| hidden(centre, face)),
                    "bar {index}: its halo lies over a bar in front"
                );
            }
            let (silhouette, _) = bar_silhouette(panel, index).unwrap();
            let (all, shown) = (length(silhouette.flatten(0.5)), length(ink.flatten(0.5)));
            if shown < all - 1.0 {
                partly += 1;
            } else {
                assert!((shown - all).abs() < 1e-6, "bar {index}: {shown} of {all}");
                whole += 1;
            }
        }
        assert!(partly > 0, "the fixture has a partly hidden bar");
        assert!(whole > 0, "the fixture has a bar in full view");
    }

    #[test]
    fn bar3d_silhouette_keeps_outer_face_edges_and_removes_shared_edges() {
        let input: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/bar3d.json"
        ))
        .unwrap();
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let panel = &geometry.panels[0];
        for index in 0..panel.observations.len() {
            let mut expected: Vec<([f64; 2], [f64; 2])> = Vec::new();
            for item in &panel.items {
                if let Item::Area {
                    path,
                    role: Role::Mark,
                    obs: Some(owner),
                    ..
                } = item
                    && *owner == index
                {
                    for ring in path.flatten(0.5) {
                        for pair in ring.windows(2) {
                            let (a, b) = (pair[0], pair[1]);
                            if let Some(i) = expected
                                .iter()
                                .position(|(x, y)| (*x == a && *y == b) || (*x == b && *y == a))
                            {
                                expected.remove(i);
                            } else {
                                expected.push((a, b));
                            }
                        }
                    }
                }
            }
            let (silhouette, _) = bar_silhouette(panel, index).expect("bar has an outside");
            let actual: Vec<_> = silhouette
                .flatten(0.5)
                .into_iter()
                .flat_map(|ring| {
                    ring.windows(2)
                        .map(|pair| (pair[0], pair[1]))
                        .collect::<Vec<_>>()
                })
                .collect();
            assert!(!expected.is_empty());
            assert_eq!(
                actual.len(),
                expected.len(),
                "bar {index} has only outside edges"
            );
            for (a, b) in expected {
                assert!(
                    actual
                        .iter()
                        .any(|(x, y)| (*x == a && *y == b) || (*x == b && *y == a)),
                    "bar {index} misses an outside edge"
                );
            }
        }
    }

    #[test]
    fn map_countries_and_surface_quads_use_their_own_paths() {
        for (kind, fixture) in [
            (
                "map",
                include_str!("../../../../../richview-chart/tests/fixtures/types/map.json"),
            ),
            (
                "surface",
                include_str!("../../../../../richview-chart/tests/fixtures/types/surface.json"),
            ),
        ] {
            let input: serde_json::Value = serde_json::from_str(fixture).unwrap();
            let geometry = layout(
                &ChartBlock::from_value(&input["spec"]).unwrap(),
                input["rows"].as_array().unwrap(),
                &ChartTheme::default(),
                &Options::default(),
            );
            for panel in &geometry.panels {
                for index in 0..panel.observations.len() {
                    let paths: Vec<_> = panel
                        .items
                        .iter()
                        .filter_map(|item| match item {
                            Item::Area {
                                path,
                                role: Role::Mark,
                                obs: Some(owner),
                                ..
                            } if *owner == index => Some(path),
                            _ => None,
                        })
                        .collect();
                    assert!(!paths.is_empty(), "{kind} observation {index} has a path");
                    let outlines =
                        tiles(kind, panel, index, Rgba([0.0, 0.0, 0.0, 1.0]), 1.0).unwrap();
                    assert_eq!(
                        outlines.len(),
                        paths.len(),
                        "{kind} observation {index} has only its paths"
                    );
                    for (path, outline) in paths.into_iter().zip(outlines.iter()) {
                        assert!(
                            matches!(outline, Item::Line { path: actual, .. } if actual == path)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn map_spots_keep_their_own_point_rings() {
        let spec = serde_json::json!({"type":"map", "title":"Offices", "encoding":{"lat":"la", "lon":"lo"}});
        let rows = vec![
            serde_json::json!({"la":50.09,"lo":14.42}),
            serde_json::json!({"la":-33.87,"lo":151.21}),
        ];
        let geometry = layout(
            &ChartBlock::from_value(&spec).unwrap(),
            &rows,
            &ChartTheme::default(),
            &Options::default(),
        );
        let panel = &geometry.panels[0];
        for (index, observation) in panel.observations.iter().enumerate() {
            let outlines = items(
                "map",
                panel,
                index,
                observation.wedge,
                Rgba([0.0, 0.0, 0.0, 1.0]),
                geometry.background,
                1.0,
            )
            .unwrap();
            assert!(outlines.iter().any(
                |item| matches!(item, Item::Point { center, .. } if *center == observation.at)
            ));
        }
    }

    #[test]
    fn contour_grid_feedback_is_a_dot_at_the_selected_node() {
        let input: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/contour.json"
        ))
        .unwrap();
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let color = Rgba([0.3, 0.4, 0.5, 1.0]);
        for panel in &geometry.panels {
            for (index, observation) in panel.observations.iter().enumerate() {
                let dot = node(panel, index, 1.0, color, geometry.background).unwrap();
                assert_eq!(dot.len(), 3);
                for (item, radius) in dot.iter().zip([12.0, 8.0, 6.0]) {
                    assert!(
                        matches!(item, Item::Point { center, radius: r, shape: Shape::Circle, .. } if *center == observation.at && *r == radius)
                    );
                }
                assert!(
                    matches!(dot[0], Item::Point { fill: Some(fill), .. } if fill.color == halo(color, 1.0))
                );
            }
            assert!(
                node(
                    panel,
                    panel.observations.len(),
                    1.0,
                    color,
                    geometry.background
                )
                .is_none()
            );
        }
    }

    #[test]
    fn network_nodes_keep_their_own_point_rings() {
        let input: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/network.json"
        ))
        .unwrap();
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let panel = &geometry.panels[0];
        let nodes: Vec<_> = panel
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Point {
                    center,
                    role: Role::Mark,
                    obs: Some(index),
                    ..
                } => Some((*index, *center)),
                _ => None,
            })
            .collect();
        assert!(!nodes.is_empty());
        for (index, center) in nodes {
            let outlines = items(
                "network",
                panel,
                index,
                None,
                Rgba([0.0, 0.0, 0.0, 1.0]),
                geometry.background,
                1.0,
            )
            .unwrap();
            assert!(outlines.iter().any(
                |item| matches!(item, Item::Point { center: actual, .. } if *actual == center)
            ));
            assert!(outlines.iter().all(
                |item| !matches!(item, Item::Point { center: actual, .. } if *actual != center)
            ));
        }
    }

    #[test]
    fn gauge_contours_the_band_named_by_the_needle() {
        let mut input: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/gauge.json"
        ))
        .unwrap();
        for (value, expected) in [(25, 0), (50, 1), (72, 1), (100, 2)] {
            input["rows"][0]["score"] = serde_json::json!(value);
            let geometry = layout(
                &ChartBlock::from_value(&input["spec"]).unwrap(),
                input["rows"].as_array().unwrap(),
                &ChartTheme::default(),
                &Options::default(),
            );
            let panel = &geometry.panels[0];
            assert!(
                richview_chart::hit::at(&geometry, panel.observations[0].at).is_some(),
                "the gauge needle stays reachable at {value}"
            );
            assert!(
                richview_chart::hit::at(&geometry, panel.observations[0].area.unwrap().center())
                    .is_some(),
                "the gauge retains its original pointer area at {value}"
            );
            let bands: Vec<_> = panel
                .items
                .iter()
                .filter(|item| {
                    matches!(
                        item,
                        Item::Area {
                            role: Role::Mark,
                            ..
                        }
                    )
                })
                .take(3)
                .collect();
            assert_eq!(bands.len(), 3);
            let outlines = items(
                "gauge",
                panel,
                0,
                None,
                Rgba([0.0, 0.0, 0.0, 1.0]),
                geometry.background,
                1.0,
            )
            .unwrap();
            assert!(
                matches!((bands[expected], outlines.as_slice()), (Item::Area { path, .. }, [Item::Line { path: actual, .. }]) if path == actual)
            );
        }
        input["spec"]["style"]
            .as_object_mut()
            .unwrap()
            .remove("bands");
        input["rows"][0]["score"] = serde_json::json!(0);
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let panel = &geometry.panels[0];
        let track = panel
            .items
            .iter()
            .find_map(|item| match item {
                Item::Area {
                    path,
                    role: Role::Band,
                    ..
                } => Some(path),
                _ => None,
            })
            .unwrap();
        let outlines = items(
            "gauge",
            panel,
            0,
            None,
            Rgba([0.0, 0.0, 0.0, 1.0]),
            geometry.background,
            1.0,
        )
        .unwrap();
        assert!(matches!(outlines.as_slice(), [Item::Line { path, .. }] if path == track));
    }

    #[test]
    fn flow_card_contour_traces_the_card_outline() {
        let input: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/flow.json"
        ))
        .unwrap();
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let mut checked = 0;
        for panel in &geometry.panels {
            for (index, observation) in panel.observations.iter().enumerate() {
                // A card is a filled shape with its outline; a connection is a line alone.
                let filled = panel.items.iter().any(|item| {
                    matches!(item, Item::Area { role: Role::Mark, obs: Some(owner), .. } if *owner == index)
                });
                let Some(outline) = panel.items.iter().find_map(|item| match item {
                    Item::Line {
                        path,
                        role: Role::Mark,
                        obs: Some(owner),
                        ..
                    } if *owner == index && filled => Some(path),
                    _ => None,
                }) else {
                    continue;
                };
                checked += 1;
                let contour = items(
                    "flow",
                    panel,
                    index,
                    observation.wedge,
                    Rgba([0.0, 0.0, 0.0, 1.0]),
                    geometry.background,
                    1.0,
                )
                .unwrap();
                assert!(
                    contour
                        .iter()
                        .any(|item| matches!(item, Item::Line { path, .. } if path == outline)),
                    "card {index} is traced along its own outline"
                );
            }
        }
        assert!(checked > 0, "the fixture has cards");
    }

    fn follows_mark(kind: &str, fixture: &str, overlap: bool) {
        let input: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let panel = &geometry.panels[0];
        let observation = panel
            .observations
            .iter()
            .position(|o| !overlap || o.readout.lines.len() > 1)
            .unwrap();
        let path = panel
            .items
            .iter()
            .find_map(|item| match item {
                Item::Area {
                    path,
                    obs: Some(i),
                    role: Role::Mark,
                    ..
                } if *i == observation => Some(path),
                _ => None,
            })
            .unwrap();
        let ink = Rgba([0.2, 0.2, 0.2, 1.0]);
        let outlines = if kind == "hexbin" {
            tiles(kind, panel, observation, ink, 1.0)
        } else {
            items(
                kind,
                panel,
                observation,
                None,
                ink,
                geometry.background,
                1.0,
            )
        }
        .expect("the mark has an outline");
        let expected = if kind == "hexbin" {
            ink.with_alpha(0.85)
        } else {
            ink
        };
        assert!(
            matches!(outlines.as_slice(), [Item::Line { path: actual, stroke, .. }] if actual == path && *stroke == Stroke::solid(expected, 1.5)),
            "outline must follow the mark path using the bin stroke"
        );
    }

    #[test]
    fn hexbin_outline_follows_hexagon() {
        follows_mark(
            "hexbin",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../richview-chart/tests/fixtures/types/hexbin.json"
            )),
            false,
        );
    }

    #[test]
    fn venn_outline_follows_overlap_boundary() {
        follows_mark(
            "venn",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../richview-chart/tests/fixtures/types/venn.json"
            )),
            true,
        );
    }
    #[test]
    fn chord_outlines_follow_the_selected_arc_and_ribbon() {
        let input: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../richview-chart/tests/fixtures/types/chord.json"
        )))
        .unwrap();
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let panel = &geometry.panels[0];
        let mut matched = [false; 2];
        for (observation, o) in panel.observations.iter().enumerate() {
            let Some(path) = panel.items.iter().find_map(|item| match item {
                Item::Area {
                    path,
                    role: Role::Mark,
                    obs: Some(index),
                    ..
                } if *index == observation => Some(path),
                _ => None,
            }) else {
                continue;
            };
            matched[usize::from(o.wedge.is_some())] = true;
            let ink = Rgba([0.2, 0.2, 0.2, 1.0]);
            let outlines = items(
                "chord",
                panel,
                observation,
                o.wedge,
                ink,
                geometry.background,
                1.0,
            )
            .unwrap();
            assert!(
                outlines
                    .iter()
                    .any(|item| matches!(item, Item::Line { path: actual, .. } if actual == path)),
                "observation {observation}"
            );
        }
        assert_eq!(matched, [true, true]);
    }
}
