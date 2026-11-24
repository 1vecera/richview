//! What a hovered or selected mark keeps at full strength while the rest of its chart steps
//! back (FEA-CH-52). Charts where every mark is being read keep everything: lines and areas
//! at the nearest x, and tiles, bins and glyphs that share one series.
//!
//! Where keyboard focus shows (STY-38): the frame's ring on arrival, kept 3 px from the chart's
//! text, and a ring around the mark an arrow key selected, which replaces it.

#[cfg(target_arch = "wasm32")]
use super::View;
use crate::mesh::{DIM_FLOOR, FLOW_DIM_FLOOR, Focus, POINT_DIM_FLOOR, connected, layer_at};
use richview_chart::geometry::{
    Cmd, Fill, Geometry, Item, Path, Rect, Rgba, Role, Shape, Targeting,
};
use richview_chart::hit::Target;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// How far a contour's 1.5 px ink line reaches past the edge of the mark it follows.
const CONTOUR_REACH: f64 = 0.75;
/// The mark ring's distance from the contour, and its width, in CSS pixels (STY-38).
const RING_GAP: f64 = 2.0;
const RING_WIDTH: f64 = 2.0;
/// The mark's filled radius at full size.
const DOT_REACH: f64 = 6.0;
/// A streamgraph layer has no hover dot: the ring marks the selected point on it.
const LAYER_POINT_REACH: f64 = 4.0;
/// The frame ring's width, the room it keeps from text, and where it sits by default: its
/// outer edge 6 px outside the frame, so it starts 4 px out.
const FRAME_RING: f64 = 2.0;
const TEXT_ROOM: f64 = 3.0;
const FRAME_RING_INSET: f64 = -6.0;

/// Place the frame ring on keyboard arrival. A selected mark takes over, using the
/// clipped pointer ring where available and a focus outline for other targets.
#[cfg(target_arch = "wasm32")]
pub(super) fn ring(v: &View) -> Vec<Item> {
    place_frame_ring(v);
    let target = v.target.filter(|_| v.keyed);
    if v.frame.has_attribute("data-rv-selected") != target.is_some() {
        let _ = if target.is_some() {
            v.frame.set_attribute("data-rv-selected", "")
        } else {
            v.frame.remove_attribute("data-rv-selected")
        };
    }
    let Some(t) = target else {
        return Vec::new();
    };
    let Some(panel) = v.geometry.panels.get(t.panel) else {
        return Vec::new();
    };
    let kind = v.chart.block.kind.name();
    if super::contour::crisp_target(kind, panel, t.observation) {
        return Vec::new();
    }
    // A family the contour already draws, or traces tile by tile, asks for nothing at level 0.
    let clear = Rgba([0.0; 4]);
    let contoured = super::contour::items(kind, panel, t.observation, clear, 0.0, false, false)
        .is_some()
        || (panel.targeting == Targeting::Contain
            && super::contour::tiles(kind, panel, t.observation, 0.0).is_some());
    let link = web_sys::window()
        .and_then(|w| w.get_computed_style(&v.frame).ok().flatten())
        .and_then(|s| s.get_property_value("--rv-link").ok())
        .and_then(|token| richview_chart::color::parse(token.trim()))
        .or_else(|| richview_chart::color::parse(&v.chart.payload.theme.accent))
        .map_or(Rgba([0.2, 0.35, 0.9, 1.0]), |c| Rgba::from_oklch(c, 1.0));
    // A traced outline is worth keeping between frames: the arrival redraws it seven times.
    thread_local! {
        static TRACED: std::cell::RefCell<(Vec<Outline>, Vec<Vec<Lines>>)> =
            const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
    }
    // A gauge's ring traces the band its needle falls in.
    let mut outlines = if kind == "gauge" {
        let drawn = super::contour::items(kind, panel, t.observation, clear, 1.0, false, false);
        drawn_outlines(&drawn.unwrap_or_default())
    } else {
        outlines(kind, &v.geometry, t, v.pointer, contoured)
    };
    let growth = super::contour::point_growth(kind, v.chart.block.encoding.size.is_some());
    for outline in &mut outlines {
        if let Outline::Circle { radius, .. } = outline {
            *radius *= 1.0 + growth;
        }
    }
    let lines = TRACED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.0 != outlines {
            cache.1 = ring_lines(&outlines);
            cache.0 = outlines;
        }
        cache.1.clone()
    });
    mark_ring(&lines, link, super::feedback::grown(v))
}

/// Set the frame ring's four insets, once per layout: the geometry's text and the size are the
/// key, so a redraw that moves nothing measures nothing.
#[cfg(target_arch = "wasm32")]
fn place_frame_ring(v: &View) {
    let margins = text_margins(&v.geometry);
    let key = format!(
        "{:.1} {:.1} {}",
        v.geometry.width,
        v.geometry.height,
        margins.map(|m| format!("{m:.1}")).join(" ")
    );
    if v.frame.get_attribute("data-rv-ring").as_deref() == Some(key.as_str()) {
        return;
    }
    let room = page_text_room(&v.figure, &v.frame);
    let style = v.frame.style();
    for (side, inset) in ["top", "right", "bottom", "left"]
        .into_iter()
        .zip(frame_ring_insets(margins, room))
    {
        let _ = style.set_property(&format!("--rv-ring-{side}"), &format!("{inset:.1}px"));
    }
    let _ = v.frame.set_attribute("data-rv-ring", &key);
}

/// How far the figure's visible text outside the frame (title, legend, method note) keeps
/// from each side of the frame, top, right, bottom and left; a side with no text is unbounded.
#[cfg(target_arch = "wasm32")]
fn page_text_room(figure: &web_sys::Element, frame: &web_sys::HtmlElement) -> [f64; 4] {
    use wasm_bindgen::JsCast;
    let mut room = [f64::INFINITY; 4];
    let Ok(nodes) = figure.query_selector_all("*") else {
        return room;
    };
    let f = frame.get_bounding_client_rect();
    let style_of = |e: &web_sys::Element| {
        web_sys::window().and_then(|w| w.get_computed_style(e).ok().flatten())
    };
    // Text nobody sees, such as an action's tooltip before its button is hovered.
    let hidden = |element: &web_sys::Element| {
        let mut at = Some(element.clone());
        while let Some(e) = at {
            if let Some(style) = style_of(&e)
                && (style.get_property_value("opacity").as_deref() == Ok("0")
                    || style.get_property_value("visibility").as_deref() == Ok("hidden"))
            {
                return true;
            }
            if &e == figure {
                break;
            }
            at = e.parent_element();
        }
        false
    };
    for i in 0..nodes.length() {
        let Some(element) = nodes
            .item(i)
            .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
        else {
            continue;
        };
        // A script, style or template draws no text; the chart's payload script holds all its
        // rows, and reading it on every zoom step copied them into Wasm each time.
        if frame.contains(Some(element.as_ref()))
            || matches!(
                element.local_name().as_str(),
                "script" | "style" | "template"
            )
        {
            continue;
        }
        let children = element.child_nodes();
        let has_text = (0..children.length()).any(|k| {
            children.item(k).is_some_and(|n| {
                n.node_type() == web_sys::Node::TEXT_NODE
                    && n.text_content().is_some_and(|t| !t.trim().is_empty())
            })
        });
        if !has_text || hidden(&element) {
            continue;
        }
        let rects = element.get_client_rects();
        for k in 0..rects.length() {
            let Some(r) = rects.item(k) else { continue };
            if r.width() <= 0.0 || r.height() <= 0.0 {
                continue;
            }
            let across = r.right() > f.left() && r.left() < f.right();
            let along = r.bottom() > f.top() && r.top() < f.bottom();
            for (side, beside, gap) in [
                (0, across, f.top() - r.bottom()),
                (1, along, r.left() - f.right()),
                (2, across, r.top() - f.bottom()),
                (3, along, f.left() - r.right()),
            ] {
                if beside && gap >= -0.5 {
                    room[side] = room[side].min(gap.max(0.0));
                }
            }
        }
    }
    room
}

/// How far the geometry's text keeps from each canvas edge, top, right, bottom and left; a
/// side without text is unbounded. Text past an edge is cut there, so it counts as at the edge.
fn text_margins(geometry: &Geometry) -> [f64; 4] {
    let mut margins = [f64::INFINITY; 4];
    for item in geometry.items() {
        let Item::Text(t) = item else { continue };
        let b = turned(t.bounds, [t.x, t.y], t.rotate);
        for (side, room) in [
            b.y,
            geometry.width - b.right(),
            geometry.height - b.bottom(),
            b.x,
        ]
        .into_iter()
        .enumerate()
        {
            margins[side] = margins[side].min(room.max(0.0));
        }
    }
    margins
}

/// The box `rect` covers once turned `degrees` clockwise around `anchor`.
fn turned(rect: Rect, anchor: [f64; 2], degrees: f64) -> Rect {
    if degrees == 0.0 {
        return rect;
    }
    let (sin, cos) = degrees.to_radians().sin_cos();
    let corners = [
        [rect.x, rect.y],
        [rect.right(), rect.y],
        [rect.right(), rect.bottom()],
        [rect.x, rect.bottom()],
    ]
    .map(|[x, y]| {
        let (dx, dy) = (x - anchor[0], y - anchor[1]);
        [
            anchor[0] + dx * cos - dy * sin,
            anchor[1] + dx * sin + dy * cos,
        ]
    });
    let min = |axis: usize| {
        corners
            .iter()
            .map(|c| c[axis])
            .fold(f64::INFINITY, f64::min)
    };
    let max = |axis: usize| {
        corners
            .iter()
            .map(|c| c[axis])
            .fold(f64::NEG_INFINITY, f64::max)
    };
    Rect::new(min(0), min(1), max(0) - min(0), max(1) - min(1))
}

/// Where the frame ring's outer edge sits on each side, inward from the frame's edge (negative
/// is outside), given how far the canvas text keeps inside each edge (`margins`) and how far
/// the page's text keeps outside it (`room`). It stays 4 px outside where both allow, and
/// otherwise moves just far enough to keep 3 px from both; with no such place it splits the
/// difference.
fn frame_ring_insets(margins: [f64; 4], room: [f64; 4]) -> [f64; 4] {
    std::array::from_fn(|side| {
        let outermost = TEXT_ROOM - room[side];
        let innermost = margins[side] - TEXT_ROOM - FRAME_RING;
        if outermost <= innermost {
            FRAME_RING_INSET.clamp(outermost, innermost)
        } else {
            (outermost + innermost) / 2.0
        }
    })
}

/// What a mark ring keeps its gap from: the contour's outer edge, a circle or `reach` outside a
/// closed outline.
#[derive(Debug, Clone, PartialEq)]
enum Outline {
    Circle {
        center: [f64; 2],
        radius: f64,
    },
    Polygon {
        points: Vec<[f64; 2]>,
        reach: f64,
    },
    /// A glyph of several parts, such as a candle: the ring follows their contours together.
    Parts(Vec<Part>),
}

impl Outline {
    /// The closed lines `d` past this contour, for each `d` in `ds`.
    fn lines(&self, ds: &[f64]) -> Vec<Vec<Vec<[f64; 2]>>> {
        match self {
            Outline::Circle { center, radius } => ds
                .iter()
                .map(|d| vec![circle_points(*center, radius + d)])
                .collect(),
            Outline::Polygon { points, reach } => {
                ds.iter().map(|d| vec![offset(points, reach + d)]).collect()
            }
            Outline::Parts(parts) => traced(parts, ds),
        }
    }
}

/// One part of a glyph and where its contour ends.
#[derive(Debug, Clone, PartialEq)]
enum Part {
    /// A stroke's straight piece; its contour ends `reach` from it.
    Segment {
        a: [f64; 2],
        b: [f64; 2],
        reach: f64,
    },
    /// A point's contour.
    Disc { center: [f64; 2], radius: f64 },
    /// A bar or body with its corner radii; its contour ends `reach` outside it.
    Bar {
        rect: Rect,
        radius: [f64; 4],
        reach: f64,
    },
    /// A filled shape's closed outlines; its contour ends `reach` outside them.
    Shape {
        loops: Vec<Vec<[f64; 2]>>,
        reach: f64,
    },
    /// A face painted in front of the mark, such as a nearer 3-D bar's: the ring goes round only
    /// what it leaves visible, along its edge rather than across it.
    Front { face: Vec<[f64; 2]> },
}

impl Part {
    /// How far `p` lies past this part's contour, negative inside it.
    fn past(&self, p: [f64; 2]) -> f64 {
        match self {
            Part::Segment { a, b, reach } => to_segment(*a, *b, p) - reach,
            Part::Disc { center, radius } => dist(*center, p) - radius,
            Part::Bar {
                rect,
                radius,
                reach,
            } => {
                let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
                let corner = match (p[0] < cx, p[1] < cy) {
                    (true, true) => radius[0],
                    (false, true) => radius[1],
                    (false, false) => radius[2],
                    (true, false) => radius[3],
                };
                let r = corner.min(rect.w.min(rect.h) / 2.0).max(0.0);
                let qx = (p[0] - cx).abs() - (rect.w / 2.0 - r);
                let qy = (p[1] - cy).abs() - (rect.h / 2.0 - r);
                qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r - reach
            }
            Part::Shape { loops, reach } => {
                let edge = loops
                    .iter()
                    .flat_map(|l| {
                        (0..l.len()).map(move |i| to_segment(l[i], l[(i + 1) % l.len()], p))
                    })
                    .fold(f64::INFINITY, f64::min);
                let inside = loops.iter().map(|l| crossings(l, p)).sum::<usize>() % 2 == 1;
                if inside { -edge - reach } else { edge - reach }
            }
            // A face in front is no part of the mark's contour.
            Part::Front { .. } => f64::INFINITY,
        }
    }

    /// The box this part's contour stays inside.
    fn bounds(&self) -> Rect {
        let spread = |points: &mut dyn Iterator<Item = [f64; 2]>, reach: f64| {
            let [mut x0, mut y0, mut x1, mut y1] = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for p in points {
                (x0, y0, x1, y1) = (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1]));
            }
            Rect::new(x0, y0, x1 - x0, y1 - y0).inflate(reach)
        };
        match self {
            Part::Segment { a, b, reach } => spread(&mut [*a, *b].into_iter(), *reach),
            Part::Disc { center, radius } => spread(&mut std::iter::once(*center), *radius),
            Part::Bar { rect, reach, .. } => rect.inflate(*reach),
            Part::Shape { loops, reach } => spread(&mut loops.iter().flatten().copied(), *reach),
            Part::Front { face } => spread(&mut face.iter().copied(), 0.0),
        }
    }
}

/// The parts of a glyph's marks, each with where its contour ends.
fn parts(marks: &[&Item]) -> Vec<Part> {
    let mut parts = Vec::new();
    for mark in marks {
        match mark {
            Item::Point { .. } => {
                if let Some((center, radius)) = point_reach(mark) {
                    parts.push(Part::Disc { center, radius });
                }
            }
            Item::Rect { rect, radius, .. } => parts.push(Part::Bar {
                rect: *rect,
                radius: *radius,
                reach: CONTOUR_REACH,
            }),
            Item::Line { path, stroke, .. } => {
                let reach = stroke.width / 2.0 + CONTOUR_REACH;
                for line in path.flatten(0.25) {
                    if let [only] = line.as_slice() {
                        parts.push(Part::Disc {
                            center: *only,
                            radius: reach,
                        });
                    }
                    parts.extend(line.windows(2).map(|w| Part::Segment {
                        a: w[0],
                        b: w[1],
                        reach,
                    }));
                }
            }
            Item::Area { path, .. } => parts.push(Part::Shape {
                loops: path
                    .flatten(0.25)
                    .into_iter()
                    .filter(|l| l.len() > 2)
                    .collect(),
                reach: CONTOUR_REACH,
            }),
            Item::Text(_) => {}
        }
    }
    parts
}

/// The grid step for tracing a glyph's ring, and the most cells a trace may sample.
const TRACE_STEP: f64 = 0.5;
const TRACE_CELLS: f64 = 160_000.0;

/// The closed lines where the nearest of `parts` is `d` past its contour, for each `d` in
/// `levels`: the distance is sampled on a grid and each line traced by marching squares.
fn traced(parts: &[Part], levels: &[f64]) -> Vec<Vec<Vec<[f64; 2]>>> {
    let (front, parts): (Vec<&Part>, Vec<&Part>) = parts
        .iter()
        .partition(|part| matches!(part, Part::Front { .. }));
    let Some(bounds) = parts
        .iter()
        .map(|part| part.bounds())
        .reduce(|a, b| a.union(&b))
    else {
        return vec![Vec::new(); levels.len()];
    };
    let top = levels.iter().copied().fold(0.0, f64::max);
    let inner = bounds.inflate(top + 1.0);
    let step = (inner.w * inner.h / TRACE_CELLS).sqrt().max(TRACE_STEP);
    let b = inner.inflate(step);
    let (nx, ny) = ((b.w / step).ceil() as usize, (b.h / step).ceil() as usize);
    let at = |i: usize, j: usize| [b.x + i as f64 * step, b.y + j as f64 * step];
    // Each part writes only the nodes it can be nearest to below `far`; the rest stay far.
    let far = top + 1.0;
    let mut field = vec![far; (nx + 1) * (ny + 1)];
    let first = |from: f64, origin: f64, most: usize| {
        (((from - origin) / step).floor().max(0.0) as usize).min(most)
    };
    let last = |to: f64, origin: f64, most: usize| {
        (((to - origin) / step).ceil().max(0.0) as usize).min(most)
    };
    let write = |part: &Part, field: &mut [f64]| {
        let reach = part.bounds().inflate(far);
        for j in first(reach.y, b.y, ny)..=last(reach.y + reach.h, b.y, ny) {
            for i in first(reach.x, b.x, nx)..=last(reach.x + reach.w, b.x, nx) {
                let k = j * (nx + 1) + i;
                field[k] = field[k].min(part.past(at(i, j)));
            }
        }
    };
    // Visit every node strictly inside `loops`, row by row (even-odd).
    let fill = |loops: &[Vec<[f64; 2]>], visit: &mut dyn FnMut(usize)| {
        for j in 0..=ny {
            let y = b.y + j as f64 * step;
            let mut xs: Vec<f64> = loops
                .iter()
                .flat_map(|l| {
                    (0..l.len()).filter_map(move |k| {
                        let (p, q) = (l[k], l[(k + 1) % l.len()]);
                        ((p[1] > y) != (q[1] > y))
                            .then(|| p[0] + (y - p[1]) / (q[1] - p[1]) * (q[0] - p[0]))
                    })
                })
                .collect();
            xs.sort_by(f64::total_cmp);
            for &[from, to] in xs.as_chunks::<2>().0 {
                for i in first(from, b.x, nx)..=last(to, b.x, nx) {
                    let x = b.x + i as f64 * step;
                    if x > from && x < to {
                        visit(j * (nx + 1) + i);
                    }
                }
            }
        }
    };
    // Faces painted in front hide what they cover: the ring goes round only what shows.
    let faces: Vec<&Vec<[f64; 2]>> = front
        .iter()
        .filter_map(|part| match part {
            Part::Front { face } => Some(face),
            _ => None,
        })
        .collect();
    for &part in &parts {
        let Part::Shape { loops, reach } = part else {
            write(part, &mut field);
            continue;
        };
        if !faces.is_empty() {
            continue;
        }
        // A shape's edges each write only the nodes near them, and a scanline fills its
        // inside: every node testing every edge would cost a detailed coastline too much.
        for l in loops {
            for (k, a) in l.iter().enumerate() {
                let segment = Part::Segment {
                    a: *a,
                    b: l[(k + 1) % l.len()],
                    reach: *reach,
                };
                write(&segment, &mut field);
            }
        }
        fill(loops, &mut |k| field[k] = field[k].min(-1.0));
    }
    if !faces.is_empty() {
        // The shapes, less the faces in front, traced as one shape: the edge of what shows
        // runs along the shapes' own edges and, where a face cuts them, along its edge.
        let (own, reach): (Vec<&Vec<[f64; 2]>>, Vec<f64>) = parts
            .iter()
            .filter_map(|part| match part {
                Part::Shape { loops, reach } => Some(loops.iter().map(move |l| (l, *reach))),
                _ => None,
            })
            .flatten()
            .unzip();
        let reach = reach.into_iter().fold(0.0, f64::max);
        let shows = |p: [f64; 2]| within(&own, p) && !within(&faces, p);
        let edges: Vec<&Vec<[f64; 2]>> = own.iter().chain(&faces).copied().collect();
        for (a, b) in region_edge(&edges, &shows) {
            write(&Part::Segment { a, b, reach }, &mut field);
        }
        for l in &own {
            fill(std::slice::from_ref(*l), &mut |k| {
                if shows(at(k % (nx + 1), k / (nx + 1))) {
                    field[k] = field[k].min(-1.0);
                }
            });
        }
    }
    let lines: Vec<Vec<Vec<[f64; 2]>>> = levels
        .iter()
        .map(|&level| march(&field, nx, ny, level, &at))
        .collect();
    if !faces.is_empty() && lines.iter().all(Vec::is_empty) {
        // Nothing shows: a ring round the whole mark still says which one is selected.
        let own: Vec<Part> = parts.into_iter().cloned().collect();
        return traced(&own, levels);
    }
    lines
}

/// Whether `p` lies inside any of `loops`, each even-odd.
fn within(loops: &[&Vec<[f64; 2]>], p: [f64; 2]) -> bool {
    loops.iter().any(|l| crossings(l, p) % 2 == 1)
}

/// The edge of the region where `inside` holds, along the edges of `loops`: each edge split where
/// another crosses it, keeping the pieces with the region on one side only.
fn region_edge(
    loops: &[&Vec<[f64; 2]>],
    inside: &dyn Fn([f64; 2]) -> bool,
) -> Vec<([f64; 2], [f64; 2])> {
    let mut pieces = Vec::new();
    for l in loops {
        for k in 0..l.len() {
            let (a, b) = (l[k], l[(k + 1) % l.len()]);
            let d = [b[0] - a[0], b[1] - a[1]];
            let length = d[0].hypot(d[1]);
            if length < 1e-9 {
                continue;
            }
            let mut ts = vec![0.0, 1.0];
            for other in loops {
                for m in 0..other.len() {
                    ts.extend(cross_at(a, b, other[m], other[(m + 1) % other.len()]));
                }
            }
            ts.sort_by(f64::total_cmp);
            ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            let along = |t: f64| [a[0] + d[0] * t, a[1] + d[1] * t];
            let side = [-d[1] / length * 0.01, d[0] / length * 0.01];
            for w in ts.windows(2) {
                let m = along((w[0] + w[1]) / 2.0);
                if inside([m[0] + side[0], m[1] + side[1]])
                    != inside([m[0] - side[0], m[1] - side[1]])
                {
                    pieces.push((along(w[0]), along(w[1])));
                }
            }
        }
    }
    pieces
}

/// Where along the segment from `a` to `b`, from 0 to 1, it crosses the one from `c` to `d`.
fn cross_at(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> Option<f64> {
    let (r, s) = ([b[0] - a[0], b[1] - a[1]], [d[0] - c[0], d[1] - c[1]]);
    let across = r[0] * s[1] - r[1] * s[0];
    if across.abs() < 1e-12 {
        return None;
    }
    let w = [c[0] - a[0], c[1] - a[1]];
    let t = (w[0] * s[1] - w[1] * s[0]) / across;
    let u = (w[0] * r[1] - w[1] * r[0]) / across;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// The faces of other 3-D bars painted after the observation's own that still hide it, near
/// enough to its contour to meet its ring. While a bar is selected the other rows step back
/// (`select` gives bar3d `SeriesOf`), so only bars of its own row, at full strength, hide it;
/// with one row that is every bar.
fn front_faces(items: &[Item], observation: usize, own: &[Part]) -> Vec<Part> {
    let mine = |item: &Item| matches!(item, Item::Area { role: Role::Mark, obs: Some(o), .. } if *o == observation);
    let (Some(last), Some(near)) = (
        items.iter().rposition(mine),
        own.iter().map(Part::bounds).reduce(|a, b| a.union(&b)),
    ) else {
        return Vec::new();
    };
    let row = items[last].series();
    let near = near.inflate(RING_GAP + RING_WIDTH + 1.0);
    items[last + 1..]
        .iter()
        .filter_map(|item| match item {
            Item::Area {
                path,
                role: Role::Mark,
                obs: Some(o),
                series,
                ..
            } if *o != observation && *series == row => Some(path.flatten(0.25)),
            _ => None,
        })
        .flatten()
        .filter(|face| face.len() > 2)
        .map(|face| Part::Front { face })
        .filter(|part| part.bounds().overlaps(&near, 0.0))
        .collect()
}

/// The closed lines where `field`, sampled on an `nx` by `ny` cell grid at `at`, is `level`.
fn march(
    field: &[f64],
    nx: usize,
    ny: usize,
    level: f64,
    at: &dyn Fn(usize, usize) -> [f64; 2],
) -> Vec<Vec<[f64; 2]>> {
    use std::collections::BTreeMap;
    let value = |i: usize, j: usize| field[j * (nx + 1) + i] - level;
    // An edge is named by its first node and its direction: 0 right, 1 down.
    let name = |i: usize, j: usize, down: usize| 2 * (j * (nx + 1) + i) + down;
    let mut points: BTreeMap<usize, [f64; 2]> = BTreeMap::new();
    let mut links: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for j in 0..ny {
        for i in 0..nx {
            let nodes = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            let v = nodes.map(|(x, y)| value(x, y));
            // Top, right, bottom and left, each from its first node to its second.
            let edges = [
                (name(i, j, 0), 0, 1),
                (name(i + 1, j, 1), 1, 2),
                (name(i, j + 1, 0), 3, 2),
                (name(i, j, 1), 0, 3),
            ];
            let mut cut = Vec::with_capacity(4);
            for (id, u, w) in edges {
                if (v[u] < 0.0) != (v[w] < 0.0) {
                    let t = v[u] / (v[u] - v[w]);
                    let (p, q) = (at(nodes[u].0, nodes[u].1), at(nodes[w].0, nodes[w].1));
                    points.insert(id, [p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
                    cut.push(id);
                }
            }
            let pairs = match cut.as_slice() {
                [a, b] => vec![(*a, *b)],
                // A saddle: the centre decides which corners the inside joins.
                [top, right, bottom, left] => {
                    if (v[0] < 0.0) == (v.iter().sum::<f64>() < 0.0) {
                        vec![(*top, *right), (*bottom, *left)]
                    } else {
                        vec![(*left, *top), (*right, *bottom)]
                    }
                }
                _ => Vec::new(),
            };
            for (a, b) in pairs {
                links.entry(a).or_default().push(b);
                links.entry(b).or_default().push(a);
            }
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut lines = Vec::new();
    for &start in links.keys() {
        if seen.contains(&start) {
            continue;
        }
        let mut line = Vec::new();
        let (mut previous, mut current) = (usize::MAX, start);
        loop {
            seen.insert(current);
            line.push(points[&current]);
            let Some(&next) = links[&current].iter().find(|&&n| n != previous) else {
                break;
            };
            (previous, current) = (current, next);
            if seen.contains(&current) {
                break;
            }
        }
        if line.len() > 2 {
            lines.push(line);
        }
    }
    lines
}

/// How many of the closed polygon's edges a ray from `p` to the right crosses.
fn crossings(points: &[[f64; 2]], p: [f64; 2]) -> usize {
    (0..points.len())
        .filter(|&i| {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
        })
        .count()
}

/// The distance from `p` to the segment from `a` to `b`.
fn to_segment(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
    };
    dist(p, [a[0] + t * dx, a[1] + t * dy])
}

/// The outlines the mark ring follows: each hover dot on a nearest-x chart, otherwise the
/// selected observation's contour. Families the contour does not draw yet follow what the
/// feedback draws for them: the ring of a point, the box of a cell.
fn outlines(
    kind: &str,
    geometry: &Geometry,
    t: Target,
    pointer: Option<[f64; 2]>,
    contoured: bool,
) -> Vec<Outline> {
    let Some(panel) = geometry.panels.get(t.panel) else {
        return Vec::new();
    };
    let Some(o) = panel.observations.get(t.observation) else {
        return Vec::new();
    };
    let circle = |center, radius| Outline::Circle { center, radius };
    if !contoured {
        match panel.targeting {
            Targeting::NearestX if kind == "streamgraph" => {
                return pointer
                    .map(|at| circle(at, LAYER_POINT_REACH))
                    .into_iter()
                    .collect();
            }
            Targeting::NearestX if o.dots.is_empty() => return vec![circle(o.at, DOT_REACH)],
            Targeting::NearestX => {
                return o.dots.iter().map(|d| circle(d.at, DOT_REACH)).collect();
            }
            // The keyboard ring surrounds the grown marker.
            Targeting::Radius => return vec![circle(o.at, o.radius.max(4.0) * 1.25)],
            // A contour chart's grid node answers with a dot like a hover dot's.
            Targeting::Contain if kind == "contour" => return vec![circle(o.at, DOT_REACH)],
            Targeting::Contain => {
                if let Some(area) = o.area {
                    return vec![Outline::Polygon {
                        points: rect_points(area, [0.0; 4]),
                        reach: CONTOUR_REACH,
                    }];
                }
            }
        }
    }
    let marks: Vec<&Item> = panel
        .items
        .iter()
        .filter(|item| match item {
            Item::Rect { obs, role, .. }
            | Item::Line { obs, role, .. }
            | Item::Area { obs, role, .. }
            | Item::Point { obs, role, .. } => *role == Role::Mark && *obs == Some(t.observation),
            Item::Text(_) => false,
        })
        .collect();
    let area = match marks.as_slice() {
        // A region's coastline has notches a mitred offset folds over: it is traced instead.
        [Item::Area { path, .. }] if kind != "map" => outer(path),
        _ => None,
    };
    if let Some(points) = area {
        return vec![Outline::Polygon {
            points,
            reach: CONTOUR_REACH,
        }];
    }
    match marks.as_slice() {
        [] => vec![match o.area {
            Some(area) => Outline::Polygon {
                points: rect_points(area, [0.0; 4]),
                reach: 0.0,
            },
            None => circle(o.at, o.radius.max(4.0)),
        }],
        [Item::Rect { rect, radius, .. }] => vec![Outline::Polygon {
            points: rect_points(*rect, *radius),
            reach: CONTOUR_REACH,
        }],
        points if points.iter().all(|m| matches!(m, Item::Point { .. })) => points
            .iter()
            .filter_map(|m| point_reach(m).map(|(center, radius)| circle(center, radius)))
            .collect(),
        // A glyph of several parts, such as a candle: one ring around their contours together.
        marks => {
            let mut pieces = parts(marks);
            // Nearer 3-D bars are painted after this one and hide what they cover.
            if kind == "bar3d" {
                let front = front_faces(&panel.items, t.observation, &pieces);
                pieces.extend(front);
            }
            if pieces.is_empty() {
                Vec::new()
            } else {
                vec![Outline::Parts(pieces)]
            }
        }
    }
}

/// The outlines of the lines and boxes a contour draws, where their 1.5 px ink ends.
fn drawn_outlines(drawn: &[Item]) -> Vec<Outline> {
    drawn
        .iter()
        .filter_map(|item| match item {
            Item::Line { path, .. } => outer(path),
            Item::Rect { rect, radius, .. } => Some(rect_points(*rect, *radius)),
            _ => None,
        })
        .map(|points| Outline::Polygon {
            points,
            reach: CONTOUR_REACH,
        })
        .collect()
}

/// A point mark's centre and the radius of its outer edge.
fn point_reach(mark: &Item) -> Option<([f64; 2], f64)> {
    let Item::Point {
        center,
        radius,
        shape,
        stroke,
        ..
    } = mark
    else {
        return None;
    };
    let reach = match shape {
        Shape::Square => radius * 0.85 * std::f64::consts::SQRT_2,
        _ => *radius,
    };
    Some((
        *center,
        reach + stroke.as_ref().map_or(0.0, |s| s.width / 2.0),
    ))
}

/// Closed lines, each a list of points.
type Lines = Vec<Vec<[f64; 2]>>;

/// Each outline's lines at the contour's edge, past the surface gap, and past the ring.
fn ring_lines(outlines: &[Outline]) -> Vec<Vec<Lines>> {
    outlines
        .iter()
        .map(|o| o.lines(&[0.0, RING_GAP, RING_GAP + RING_WIDTH]))
        .collect()
}

/// Paint one keyboard-focus band around each outline.
fn mark_ring(lines: &[Vec<Lines>], link: Rgba, level: f64) -> Vec<Item> {
    let level = level.clamp(0.0, 1.0);
    if level <= 0.0 {
        return Vec::new();
    }
    let band = |inner: &[Vec<[f64; 2]>], outer: &[Vec<[f64; 2]>], color: Rgba| {
        let mut path = Path::new();
        for points in outer.iter().chain(inner) {
            for (i, p) in points.iter().enumerate() {
                if i == 0 {
                    path.move_to(*p);
                } else {
                    path.line_to(*p);
                }
            }
            path.close();
        }
        Item::Area {
            path,
            fill: Fill::solid(color.with_alpha(color.alpha() * level)),
            role: Role::Annotation,
            series: None,
            obs: None,
        }
    };
    lines.iter().map(|l| band(&l[1], &l[2], link)).collect()
}

/// The one closed outline of `path` that holds every other, as points; `None` for an open
/// path or separate pieces.
fn outer(path: &Path) -> Option<Vec<[f64; 2]>> {
    let mut pieces: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut current: Vec<[f64; 2]> = Vec::new();
    for cmd in &path.cmds {
        match *cmd {
            Cmd::M { p } => {
                pieces.push(std::mem::take(&mut current));
                current.push(p);
            }
            Cmd::L { p } => current.push(p),
            Cmd::C { c1, c2, p } => {
                let from = current.last().copied().unwrap_or(p);
                let length = dist(from, c1) + dist(c1, c2) + dist(c2, p);
                let steps = (length / 3.0).ceil().clamp(2.0, 32.0) as usize;
                for k in 1..=steps {
                    let t = k as f64 / steps as f64;
                    let u = 1.0 - t;
                    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                    current.push([
                        w[0] * from[0] + w[1] * c1[0] + w[2] * c2[0] + w[3] * p[0],
                        w[0] * from[1] + w[1] * c1[1] + w[2] * c2[1] + w[3] * p[1],
                    ]);
                }
            }
            Cmd::Z => pieces.push(std::mem::take(&mut current)),
        }
    }
    pieces.push(current);
    pieces.retain(|piece| piece.len() > 2);
    let bounds = |piece: &[[f64; 2]]| {
        let [mut x0, mut y0, mut x1, mut y1] = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for p in piece {
            (x0, y0, x1, y1) = (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1]));
        }
        [x0, y0, x1, y1]
    };
    let index = (0..pieces.len()).find(|&i| {
        let b = bounds(&pieces[i]);
        pieces.iter().all(|piece| {
            let c = bounds(piece);
            c[0] >= b[0] - 0.5 && c[1] >= b[1] - 0.5 && c[2] <= b[2] + 0.5 && c[3] <= b[3] + 0.5
        })
    })?;
    Some(pieces.swap_remove(index))
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// `rect`'s outline with its own rounded corners, top-left, top-right, bottom-right and
/// bottom-left, clockwise on screen.
fn rect_points(rect: Rect, radius: [f64; 4]) -> Vec<[f64; 2]> {
    let most = (rect.w.min(rect.h) / 2.0).max(0.0);
    let corners = [
        ([rect.x, rect.y], [1.0, 1.0], radius[0], PI),
        (
            [rect.right(), rect.y],
            [-1.0, 1.0],
            radius[1],
            PI + FRAC_PI_2,
        ),
        ([rect.right(), rect.bottom()], [-1.0, -1.0], radius[2], 0.0),
        ([rect.x, rect.bottom()], [1.0, -1.0], radius[3], FRAC_PI_2),
    ];
    let mut points = Vec::new();
    for (corner, inward, r, from) in corners {
        let r = r.clamp(0.0, most);
        if r <= 0.0 {
            points.push(corner);
            continue;
        }
        let center = [corner[0] + inward[0] * r, corner[1] + inward[1] * r];
        for k in 0..=6 {
            let a = from + FRAC_PI_2 * k as f64 / 6.0;
            points.push([center[0] + r * a.cos(), center[1] + r * a.sin()]);
        }
    }
    points
}

/// A circle as a closed polygon fine enough to read as round at its size.
pub(super) fn circle_points(center: [f64; 2], radius: f64) -> Vec<[f64; 2]> {
    let steps = ((radius * 2.0).ceil() as usize).clamp(24, 96);
    (0..steps)
        .map(|k| {
            let a = TAU * k as f64 / steps as f64;
            [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
        })
        .collect()
}

/// The outline `d` pixels outside the closed polygon `points`: rounded where the polygon turns
/// outward, mitred where it turns in.
pub(super) fn offset(points: &[[f64; 2]], d: f64) -> Vec<[f64; 2]> {
    let mut p: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for &q in points {
        if p.last().is_none_or(|&last| dist(last, q) > 1e-6) {
            p.push(q);
        }
    }
    while p.len() > 1 && dist(p[0], p[p.len() - 1]) <= 1e-6 {
        p.pop();
    }
    let n = p.len();
    if n < 3 || d <= 0.0 {
        return p;
    }
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    // The interior lies left of each edge for a positive area; outward is the other side.
    let side = if area >= 0.0 { 1.0 } else { -1.0 };
    let normal = |a: [f64; 2], b: [f64; 2]| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = dx.hypot(dy);
        [side * dy / length, -side * dx / length]
    };
    let mut out = Vec::with_capacity(n * 3);
    for i in 0..n {
        let (before, at, after) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
        let (n0, n1) = (normal(before, at), normal(at, after));
        let turn = side
            * ((at[0] - before[0]) * (after[1] - at[1]) - (at[1] - before[1]) * (after[0] - at[0]));
        let cos = n0[0] * n1[0] + n0[1] * n1[1];
        if turn > 1e-9 {
            let from = n0[1].atan2(n0[0]);
            let mut sweep = n1[1].atan2(n1[0]) - from;
            if sweep > PI {
                sweep -= TAU;
            } else if sweep < -PI {
                sweep += TAU;
            }
            let steps = (sweep.abs() / (PI / 12.0)).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let a = from + sweep * k as f64 / steps as f64;
                out.push([at[0] + d * a.cos(), at[1] + d * a.sin()]);
            }
        } else if cos > -0.9 {
            let k = d / (1.0 + cos);
            out.push([at[0] + (n0[0] + n1[0]) * k, at[1] + (n0[1] + n1[1]) * k]);
        } else {
            out.push([at[0] + n0[0] * d, at[1] + n0[1] * d]);
            out.push([at[0] + n1[0] * d, at[1] + n1[1] * d]);
        }
    }
    out
}

/// The emphasis `target` asks for and what the rest keeps of its strength.
#[cfg(target_arch = "wasm32")]
pub(super) fn of(v: &View, target: Option<Target>) -> Option<(Focus, f32)> {
    if v.keyed && v.chart.block.kind.name() == "streamgraph" {
        return keyboard_series(&v.geometry, target?, v.pointer?)
            .map(|s| (Focus::Series(s), DIM_FLOOR));
    }
    select(v.chart.block.kind.name(), &v.geometry, v.pointer, target)
}

#[cfg(any(target_arch = "wasm32", test))]
fn keyboard_series(geometry: &Geometry, target: Target, point: [f64; 2]) -> Option<usize> {
    geometry
        .panels
        .get(target.panel)?
        .observations
        .get(target.observation)?
        .dots
        .iter()
        .find(|dot| dot.at == point)
        .map(|dot| dot.series)
}

fn select(
    kind: &str,
    geometry: &Geometry,
    pointer: Option<[f64; 2]>,
    target: Option<Target>,
) -> Option<(Focus, f32)> {
    let t = target?;
    let panel = geometry.panels.get(t.panel)?;
    let o = panel.observations.get(t.observation)?;
    match kind {
        // A flow keeps the hovered ribbon or node and everything it connects.
        "sankey" | "alluvial" => Some((
            Focus::Marks {
                panel: t.panel,
                keep: connected(panel, t.observation),
            },
            FLOW_DIM_FLOOR,
        )),
        "flow"
            if !panel.items.iter().any(|item| {
                matches!(item,
                    Item::Area { role: Role::Mark, obs: Some(i), .. }
                        | Item::Rect { role: Role::Mark, obs: Some(i), .. } if *i == t.observation
                )
            }) =>
        {
            Some((
                Focus::Marks {
                    panel: t.panel,
                    keep: vec![t.observation],
                },
                FLOW_DIM_FLOOR,
            ))
        }
        // Keep the selected parallel-coordinate line while its neighbours step back.
        "parallel" => Some((
            Focus::Marks {
                panel: t.panel,
                keep: vec![t.observation],
            },
            DIM_FLOOR,
        )),
        // One wedge among its siblings. A sunburst's rings are a hierarchy, where a wedge's
        // parents and children are read with it: nothing steps back there.
        "pie" | "rose" if panel.observations.len() > 1 => Some((
            Focus::Marks {
                panel: t.panel,
                keep: vec![t.observation],
            },
            DIM_FLOOR,
        )),
        // The mark's series among the others; marks of its own series stay, or every pointer
        // move along a series would flash the chart.
        "combo" if geometry.series.len() > 1 => {
            let mut series: Vec<_> = o
                .readout
                .lines
                .iter()
                .filter_map(|line| line.series)
                .collect();
            if let Some(own) = o.series
                && !series.contains(&own)
            {
                series.push(own);
            }
            (!series.is_empty()).then_some((Focus::SeriesSet(series), DIM_FLOOR))
        }
        // A 3-D bar's row is its series: the rows in front turn translucent and the selected
        // bar shows through them.
        kind @ ("scatter" | "bar3d" | "ribbon" | "chord" | "lollipop" | "waffle") => {
            let series = o.series?;
            let floor = if kind == "scatter" {
                POINT_DIM_FLOOR
            } else {
                DIM_FLOOR
            };
            if geometry.series.len() > 1 {
                Some((
                    Focus::SeriesOf {
                        series,
                        panel: t.panel,
                        observation: t.observation,
                    },
                    floor,
                ))
            } else {
                None
            }
        }
        // The layer under the pointer or keyboard's selected dot among the others.
        "streamgraph" => {
            let series = layer_at(panel, pointer?)?;
            Some((Focus::Series(series), DIM_FLOOR))
        }
        _ => None,
    }
}

/// Pick a streamgraph layer at `target`, starting at the top and retaining the nearest y when
/// moving across x. Up/down move through layers at that x.
#[cfg(any(target_arch = "wasm32", test))]
pub(super) fn keyboard_layer(
    geometry: &Geometry,
    target: Target,
    previous: Option<[f64; 2]>,
    direction: i32,
) -> Option<[f64; 2]> {
    let observation = geometry
        .panels
        .get(target.panel)?
        .observations
        .get(target.observation)?;
    let mut dots: Vec<_> = observation.dots.iter().map(|dot| dot.at).collect();
    dots.sort_by(|a, b| a[1].total_cmp(&b[1]));
    let nearest = previous.map_or(0, |point| {
        dots.iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (a[1] - point[1]).abs().total_cmp(&(b[1] - point[1]).abs()))
            .map_or(0, |(index, _)| index)
    });
    let index = (nearest as i32 + direction).clamp(0, dots.len().saturating_sub(1) as i32) as usize;
    dots.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use richview_chart::block::ChartBlock;
    use richview_chart::geometry::{Item, Role};
    use richview_chart::layout::{ChartTheme, Options, layout};

    fn chart(fixture: &str) -> Geometry {
        let input: serde_json::Value = serde_json::from_str(fixture).unwrap();
        layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        )
    }

    #[test]
    fn parallel_keeps_the_selected_line_and_dims_other_lines() {
        let g = chart(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../richview-chart/tests/fixtures/types/parallel.json"
        )));
        let (focus, floor) = select(
            "parallel",
            &g,
            None,
            Some(Target {
                panel: 0,
                observation: 0,
            }),
        )
        .expect("parallel has emphasis");
        assert_eq!(floor, DIM_FLOOR);
        let lines: Vec<_> = g.panels[0]
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    Item::Line {
                        role: Role::Mark,
                        obs: Some(_),
                        ..
                    }
                )
            })
            .collect();
        assert!(lines.len() > 1);
        assert!(lines.iter().any(|line| focus.keeps(0, line)));
        assert!(lines.iter().any(|line| !focus.keeps(0, line)));
        for line in lines {
            assert_eq!(
                focus.keeps(0, line),
                matches!(line, Item::Line { obs: Some(0), .. })
            );
        }
    }

    #[test]
    fn streamgraph_keyboard_changes_layers_and_keeps_the_nearest_at_the_next_x() {
        let g = chart(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../richview-chart/tests/fixtures/types/streamgraph.json"
        )));
        let first = Target {
            panel: 0,
            observation: 5,
        };
        let next = Target {
            panel: 0,
            observation: 6,
        };
        let top = keyboard_layer(&g, first, None, 0).unwrap();
        let edge = Target {
            panel: 0,
            observation: 0,
        };
        let edge_point = keyboard_layer(&g, edge, None, 0).unwrap();
        assert!(
            keyboard_series(&g, edge, edge_point).is_some(),
            "the first x selects its layer even on the polygon edge"
        );
        assert_eq!(
            top[1],
            g.panels[0].observations[5]
                .dots
                .iter()
                .map(|d| d.at[1])
                .min_by(f64::total_cmp)
                .unwrap()
        );
        let lower = keyboard_layer(&g, first, Some(top), 1).unwrap();
        assert!(lower[1] > top[1]);
        assert_eq!(keyboard_layer(&g, first, Some(lower), -1), Some(top));
        let across = keyboard_layer(&g, next, Some(lower), 0).unwrap();
        assert!(
            g.panels[0].observations[6]
                .dots
                .iter()
                .all(|d| (across[1] - lower[1]).abs() <= (d.at[1] - lower[1]).abs())
        );
        assert!(select("streamgraph", &g, Some(top), Some(first)).is_some());
        assert_ne!(
            select("streamgraph", &g, Some(top), Some(first)),
            select("streamgraph", &g, Some(lower), Some(first))
        );
    }

    #[test]
    fn ribbon_keeps_its_group_and_dims_other_groups_to_twenty_percent() {
        let g = chart(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../richview-chart/tests/fixtures/types/ribbon.json"
        )));
        let panel = &g.panels[0];
        let target = Target {
            panel: 0,
            observation: 0,
        };
        let group = panel.observations[0].series.unwrap();
        let (focus, floor) =
            select("ribbon", &g, None, Some(target)).expect("ribbon emphasizes a group");
        assert_eq!(floor, 0.2);
        let marks: Vec<_> = panel
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    Item::Rect {
                        role: Role::Mark,
                        ..
                    } | Item::Area {
                        role: Role::Band,
                        ..
                    }
                )
            })
            .filter(|i| i.series().is_some())
            .collect();
        assert!(
            marks
                .iter()
                .any(|i| matches!(i, Item::Area { .. }) && i.series() == Some(group))
        );
        assert!(marks.iter().any(|i| i.series() != Some(group)));
        for item in marks {
            assert_eq!(focus.keeps(0, item), item.series() == Some(group));
        }
    }

    #[test]
    fn combo_keeps_every_readout_series_including_the_line() {
        let g = chart(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../richview-chart/tests/fixtures/types/combo.json"
        )));
        let panel = &g.panels[0];
        let target = Target {
            panel: 0,
            observation: 0,
        };
        let listed: Vec<_> = panel.observations[0]
            .readout
            .lines
            .iter()
            .filter_map(|l| l.series)
            .collect();
        assert!(listed.len() >= 2);
        let (focus, _) = select("combo", &g, None, Some(target)).unwrap();
        let line = panel.items.iter().find(|item| matches!(item, Item::Line { role: Role::Mark, series: Some(s), .. } if listed.contains(s))).expect("listed line exists");
        assert!(
            focus.keeps(0, line),
            "a series named in the readout stays at full strength"
        );
    }
    /// The distance from `p` to the closed polygon's outline.
    fn to_outline(points: &[[f64; 2]], p: [f64; 2]) -> f64 {
        (0..points.len())
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % points.len()]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let length = dx * dx + dy * dy;
                let t = if length == 0.0 {
                    0.0
                } else {
                    (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
                };
                dist(p, [a[0] + t * dx, a[1] + t * dy])
            })
            .fold(f64::INFINITY, f64::min)
    }

    #[test]
    fn the_frame_ring_sits_four_pixels_out_unless_text_needs_it_elsewhere() {
        // Tick labels flush with the left edge, a 12 px foot, a legend 20 px above, a method
        // note flush below.
        let insets = frame_ring_insets(
            [1.5, 40.0, 12.0, 0.0],
            [20.0, f64::INFINITY, 0.0, f64::INFINITY],
        );
        assert_eq!(
            insets[0], FRAME_RING_INSET,
            "room on both sides keeps the 4 px offset"
        );
        assert_eq!(insets[1], FRAME_RING_INSET);
        assert_eq!(insets[3], FRAME_RING_INSET);
        assert_eq!(
            insets[2], TEXT_ROOM,
            "text flush below moves the ring 3 px inside"
        );
        for (inset, margin, room) in [(insets[0], 1.5, 20.0), (insets[2], 12.0, 0.0)] {
            assert!(
                margin - (inset + FRAME_RING) >= TEXT_ROOM,
                "3 px from the canvas text"
            );
            assert!(room + inset >= TEXT_ROOM, "3 px from the page's text");
        }
        // Text too close on both sides: the ring splits the difference.
        let squeezed = frame_ring_insets([1.0; 4], [4.0; 4]);
        assert_eq!(
            squeezed[0],
            (TEXT_ROOM - 4.0 + 1.0 - TEXT_ROOM - FRAME_RING) / 2.0
        );
    }

    #[test]
    fn an_offset_keeps_its_distance_round_convex_corners_and_mitred_inner_ones() {
        // An L, clockwise on screen, with one inner corner at (10, 10).
        let shape = [
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 10.0],
            [10.0, 10.0],
            [10.0, 20.0],
            [0.0, 20.0],
        ];
        for points in [shape.to_vec(), shape.iter().rev().copied().collect()] {
            let out = offset(&points, 3.0);
            for p in &out {
                assert!(
                    (to_outline(&points, *p) - 3.0).abs() < 1e-6,
                    "{p:?} is 3 px out"
                );
            }
            assert!(
                out.iter().any(|p| dist(*p, [13.0, 13.0]) < 1e-6),
                "the inner corner is mitred"
            );
            assert!(
                out.iter()
                    .all(|p| p[0] >= -3.0 - 1e-9 && p[1] >= -3.0 - 1e-9)
            );
        }
        assert_eq!(offset(&shape, 0.0), shape.to_vec());
    }

    #[test]
    fn a_rotated_label_counts_by_the_box_it_covers() {
        let b = turned(Rect::new(10.0, -8.0, 40.0, 10.0), [10.0, 0.0], -90.0);
        assert!((b.x - 2.0).abs() < 1e-9 && (b.w - 10.0).abs() < 1e-9);
        assert!((b.y + 40.0).abs() < 1e-9 && (b.h - 40.0).abs() < 1e-9);
        let g = chart(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../richview-chart/tests/fixtures/types/band.json"
        )));
        let margins = text_margins(&g);
        assert!(margins.iter().all(|m| *m >= 0.0 && m.is_finite()));
    }

    /// The selected mark's ring lies 2 to 4 px past its contour and never over the mark.
    fn ring_clears(kind: &str, fixture: &str, contoured: bool, expect: usize) {
        let g = chart(fixture);
        let panel = &g.panels[0];
        let index = panel.observations.len() / 2;
        let t = Target {
            panel: 0,
            observation: index,
        };
        let outlines = outlines(
            kind,
            &g,
            t,
            panel.observations[index].dots.first().map(|d| d.at),
            contoured,
        );
        assert_eq!(outlines.len(), expect, "{kind}: {outlines:?}");
        let link = Rgba([0.1, 0.2, 0.9, 1.0]);
        let items = mark_ring(&ring_lines(&outlines), link, 1.0);
        assert_eq!(items.len(), outlines.len(), "{kind}: one ring per outline");
        for (outline, ring) in outlines.iter().zip(&items) {
            let Item::Area { path, fill, .. } = ring else {
                panic!("{kind}: the ring is a filled band")
            };
            assert_eq!(fill.color, link);
            let points: Vec<[f64; 2]> = path
                .cmds
                .iter()
                .filter_map(|c| match c {
                    Cmd::M { p } | Cmd::L { p } => Some(*p),
                    _ => None,
                })
                .collect();
            for p in points {
                // In a notch between short edges, such as a coastline's, a mitred corner or a
                // traced line sampled on a grid strays a little outward; never towards the mark.
                let (d, inward, outward) = match outline {
                    Outline::Circle { center, radius } => (dist(*center, p) - radius, 0.05, 0.05),
                    Outline::Polygon { points, reach } => {
                        (to_outline(points, p) - reach, 0.05, 0.5)
                    }
                    Outline::Parts(parts) => (past(parts, p), 0.1, 0.75),
                };
                assert!(
                    d > RING_GAP - inward && d < RING_GAP + RING_WIDTH + outward,
                    "{kind}: a ring point {d} px past the contour"
                );
            }
        }
        assert!(mark_ring(&ring_lines(&outlines), link, 0.0).is_empty());
    }

    /// How far `p` lies past the nearest part's contour, or, with faces in front, past the
    /// edge of what the faces leave showing.
    fn past(parts: &[Part], p: [f64; 2]) -> f64 {
        let faces = front_of(parts);
        if faces.is_empty() {
            return parts
                .iter()
                .map(|q| q.past(p))
                .fold(f64::INFINITY, f64::min);
        }
        let own: Vec<&Vec<[f64; 2]>> = parts
            .iter()
            .filter_map(|part| match part {
                Part::Shape { loops, .. } => Some(loops.iter()),
                _ => None,
            })
            .flatten()
            .collect();
        let shows = |q: [f64; 2]| within(&own, q) && !within(&faces, q);
        let edges: Vec<&Vec<[f64; 2]>> = own.iter().chain(&faces).copied().collect();
        let edge = region_edge(&edges, &shows)
            .iter()
            .map(|(a, b)| to_segment(*a, *b, p))
            .fold(f64::INFINITY, f64::min);
        if shows(p) {
            -edge - CONTOUR_REACH
        } else {
            edge - CONTOUR_REACH
        }
    }

    fn front_of(parts: &[Part]) -> Vec<&Vec<[f64; 2]>> {
        parts
            .iter()
            .filter_map(|part| match part {
                Part::Front { face } => Some(face),
                _ => None,
            })
            .collect()
    }

    /// The bar3d fixture with its three rows of bars, or as one row without `color`.
    fn bar3d(rows: bool) -> Geometry {
        let mut input: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../richview-chart/tests/fixtures/types/bar3d.json"
        )))
        .unwrap();
        if !rows {
            input["spec"]["encoding"]
                .as_object_mut()
                .unwrap()
                .remove("color");
        }
        layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        )
    }

    /// The outermost line of each ring.
    fn outer_ring(outlines: &[Outline]) -> Vec<Vec<[f64; 2]>> {
        ring_lines(outlines)
            .into_iter()
            .flat_map(|levels| levels.into_iter().next_back())
            .flatten()
            .collect()
    }

    #[test]
    fn a_selected_bar3d_bar_dims_the_other_rows_and_one_row_dims_nothing() {
        let g = bar3d(true);
        let panel = &g.panels[0];
        for (observation, o) in panel.observations.iter().enumerate() {
            let t = Target {
                panel: 0,
                observation,
            };
            let (focus, floor) =
                select("bar3d", &g, None, Some(t)).expect("the other rows step back");
            assert_eq!(floor, DIM_FLOOR);
            assert!(
                matches!(focus, Focus::SeriesOf { series, .. } if Some(series) == o.series),
                "bar3d {observation}: {focus:?}"
            );
            // Faces and their hairlines step back with their row; the box, ticks and labels stay.
            for item in &panel.items {
                assert_eq!(
                    focus.keeps(0, item),
                    item.series().is_none_or(|s| Some(s) == o.series),
                    "bar3d {observation}: {item:?}"
                );
            }
            assert!(
                panel
                    .items
                    .iter()
                    .any(|item| item.role() == Role::Grid && !focus.keeps(0, item)),
                "bar3d {observation}: the other rows' hairlines step back with their faces"
            );
        }
        let one = bar3d(false);
        let t = Target {
            panel: 0,
            observation: 0,
        };
        assert_eq!(select("bar3d", &one, None, Some(t)), None);
    }

    #[test]
    fn a_hidden_bar3d_ring_goes_round_only_what_shows() {
        // With one row nothing steps back and every nearer bar hides; with several, the bars of
        // the selected one's own row do.
        for rows in [false, true] {
            hidden_bar3d_ring_goes_round_only_what_shows(&bar3d(rows), rows);
        }
    }

    fn hidden_bar3d_ring_goes_round_only_what_shows(g: &Geometry, rows: bool) {
        let panel = &g.panels[0];
        let mut hidden = Vec::new();
        let mut crossing = 0.0_f64;
        for observation in 0..panel.observations.len() {
            let t = Target {
                panel: 0,
                observation,
            };
            let outlines = outlines("bar3d", g, t, None, true);
            let [Outline::Parts(parts)] = outlines.as_slice() else {
                panic!("bar3d {observation}: one traced outline, {outlines:?}")
            };
            let faces = front_of(parts);
            let own: Vec<Part> = parts
                .iter()
                .filter(|part| !matches!(part, Part::Front { .. }))
                .cloned()
                .collect();
            // Only a bar some face in front really covers counts.
            let covered = own.iter().any(|part| match part {
                Part::Shape { loops, .. } => loops.iter().flatten().any(|v| within(&faces, *v)),
                _ => false,
            });
            if !covered {
                continue;
            }
            let rings = outer_ring(&outlines);
            let full = outer_ring(&[Outline::Parts(own.clone())]);
            if past(parts, [0.0, 0.0]).is_infinite() {
                // Nothing of it shows: the ring goes round the whole bar.
                assert_eq!(
                    rings, full,
                    "bar3d {observation}: a hidden bar keeps a ring"
                );
                continue;
            }
            hidden.push((observation, rings.len()));
            for p in rings.iter().flatten() {
                let d = past(parts, *p);
                assert!(
                    d > RING_GAP + RING_WIDTH - 0.1 && d < RING_GAP + RING_WIDTH + 0.75,
                    "bar3d {observation}: the ring is {d} px past what shows"
                );
            }
            // How far the full silhouette's ring would run into the bar in front.
            let deepest = full
                .iter()
                .flatten()
                .filter(|p| within(&faces, **p))
                .map(|p| past(parts, *p))
                .fold(0.0, f64::max);
            crossing = crossing.max(deepest);
        }
        // The whole silhouette's ring would run into the bar in front past the ring's own reach.
        assert!(
            crossing > RING_GAP + RING_WIDTH,
            "rows {rows}: the full silhouette's ring runs only {crossing} px past what shows"
        );
        assert!(
            !hidden.is_empty(),
            "rows {rows}: the fixture has a bar partly behind another"
        );
        assert!(
            hidden.iter().all(|&(_, loops)| loops == 1),
            "rows {rows}: each partly hidden bar gets one closed ring: {hidden:?}"
        );
    }

    #[test]
    fn a_selected_bar3d_ring_goes_over_the_rows_that_step_back() {
        let g = bar3d(true);
        let panel = &g.panels[0];
        let (mut whole, mut over) = (0, 0);
        for (observation, o) in panel.observations.iter().enumerate() {
            let t = Target {
                panel: 0,
                observation,
            };
            let outlines = outlines("bar3d", &g, t, None, true);
            let [Outline::Parts(parts)] = outlines.as_slice() else {
                panic!("bar3d {observation}: one traced outline, {outlines:?}")
            };
            // The faces of the other rows painted after it, dimmed while it is selected.
            let mine = |item: &Item| matches!(item, Item::Area { role: Role::Mark, obs: Some(k), .. } if *k == observation);
            let last = panel.items.iter().rposition(mine).unwrap();
            let dimmed: Vec<Vec<[f64; 2]>> = panel.items[last + 1..]
                .iter()
                .filter_map(|item| match item {
                    Item::Area {
                        path,
                        role: Role::Mark,
                        series,
                        ..
                    } if *series != o.series => Some(path.flatten(0.25)),
                    _ => None,
                })
                .flatten()
                .collect();
            let dimmed: Vec<&Vec<[f64; 2]>> = dimmed.iter().collect();
            for face in front_of(parts) {
                assert!(
                    !dimmed.contains(&face),
                    "bar3d {observation}: a dimmed row trims its ring"
                );
            }
            let own: Vec<Part> = parts
                .iter()
                .filter(|part| !matches!(part, Part::Front { .. }))
                .cloned()
                .collect();
            let covered = own.iter().any(|part| match part {
                Part::Shape { loops, .. } => loops.iter().flatten().any(|v| within(&dimmed, *v)),
                _ => false,
            });
            let rings = outer_ring(&outlines);
            if covered && front_of(parts).is_empty() {
                // Only dimmed rows lie in front of it: the ring goes round the whole bar.
                assert_eq!(
                    rings,
                    outer_ring(&[Outline::Parts(own)]),
                    "bar3d {observation}: the ring goes round the whole bar"
                );
                whole += 1;
            }
            if rings.iter().flatten().any(|p| within(&dimmed, *p)) {
                over += 1;
            }
        }
        assert!(
            whole > 0,
            "a bar only the dimmed rows cover keeps its whole ring"
        );
        assert!(
            over > 0,
            "a ring runs over the dimmed rows in front of its bar"
        );
    }

    #[test]
    fn a_glyph_ring_follows_its_strokes_and_leaves_the_empty_hit_area_alone() {
        // The runtime check (verify-report-runtime) probes the side of the selected period's hit
        // area farthest from its strokes and expects the keyboard's feedback to leave it alone.
        let g = chart(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../richview-chart/tests/fixtures/types/ohlc.json"
        )));
        let panel = &g.panels[0];
        let index = 4.min(panel.observations.len() - 1);
        let t = Target {
            panel: 0,
            observation: index,
        };
        let outlines = outlines("ohlc", &g, t, None, true);
        let [Outline::Parts(parts)] = outlines.as_slice() else {
            panic!("one ring around the glyph's parts: {outlines:?}")
        };
        let a = panel.observations[index].area.expect("a period's hit area");
        let segments: Vec<_> = parts
            .iter()
            .filter_map(|p| match p {
                Part::Segment { a, b, .. } => Some((*a, *b)),
                _ => None,
            })
            .collect();
        assert_eq!(segments.len(), 3, "the wick and both ticks");
        let side = [
            [a.x, a.y + a.h / 2.0],
            [a.x + a.w, a.y + a.h / 2.0],
            [a.x + a.w / 2.0, a.y],
            [a.x + a.w / 2.0, a.y + a.h],
        ]
        .into_iter()
        .map(|p| {
            let d = segments
                .iter()
                .map(|(u, v)| to_segment(*u, *v, p))
                .fold(f64::INFINITY, f64::min);
            (p, d)
        })
        .max_by(|x, y| x.1.total_cmp(&y.1))
        .unwrap();
        let lines = outlines[0].lines(&[RING_GAP + RING_WIDTH]);
        let reach = lines[0]
            .iter()
            .flatten()
            .map(|p| {
                segments
                    .iter()
                    .map(|(u, v)| to_segment(*u, *v, *p))
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max);
        assert!(
            side.1 >= reach + 0.75,
            "the ring reaches {reach} px from the strokes, the empty side is {} px away",
            side.1
        );
    }

    #[test]
    fn a_gauge_ring_follows_the_band_its_contour_draws() {
        let fixture = std::fs::read_to_string(format!(
            "{}/../../richview-chart/tests/fixtures/types/gauge.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let g = chart(&fixture);
        let panel = &g.panels[0];
        let clear = Rgba([0.0; 4]);
        let drawn = crate::test_contour::items("gauge", panel, 0, clear, 1.0, false, false)
            .expect("the contour draws a gauge's band");
        let outlines = drawn_outlines(&drawn);
        let [
            Outline::Polygon {
                points: band,
                reach,
            },
        ] = outlines.as_slice()
        else {
            panic!("one band: {outlines:?}")
        };
        let link = Rgba([0.1, 0.2, 0.9, 1.0]);
        let items = mark_ring(&ring_lines(&outlines), link, 1.0);
        let Some(Item::Area { path, .. }) = items.last() else {
            panic!("the ring is a filled band")
        };
        let points: Vec<[f64; 2]> = path
            .cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::M { p } | Cmd::L { p } => Some(*p),
                _ => None,
            })
            .collect();
        for p in &points {
            let d = to_outline(band, *p) - reach;
            assert!(
                d > RING_GAP - 0.05 && d < RING_GAP + RING_WIDTH + 0.5,
                "a ring point {d} px past the band's contour"
            );
        }
        // Not the box around the whole dial.
        let xs = points.iter().map(|p| p[0]);
        let width = xs.clone().fold(f64::MIN, f64::max) - xs.fold(f64::MAX, f64::min);
        assert!(
            width < panel.plot.w / 2.0,
            "the ring spans {width} of {}",
            panel.plot.w
        );
    }

    #[test]
    fn a_point_ring_is_whole_and_concentric_across_the_axis() {
        let plot = Rect::new(10.0, 0.0, 40.0, 40.0);
        let center = [plot.x, 20.0];
        let outlines = [Outline::Circle {
            center,
            radius: 6.0,
        }];
        let lines = ring_lines(&outlines);
        for (index, radius) in [(1, 6.0 + RING_GAP), (2, 6.0 + RING_GAP + RING_WIDTH)] {
            let points = &lines[0][index][0];
            assert!(
                points
                    .iter()
                    .all(|p| (dist(*p, center) - radius).abs() < 1e-8)
            );
            let edges = points.iter().map(|p| p[0]);
            assert_eq!(edges.clone().fold(f64::MAX, f64::min), plot.x - radius);
            assert_eq!(edges.fold(f64::MIN, f64::max), plot.x + radius);
        }
        let rings = mark_ring(&lines, Rgba([0.1, 0.2, 0.9, 1.0]), 1.0);
        let [Item::Area { path, .. }] = rings.as_slice() else {
            panic!("one ring around the point");
        };
        assert_eq!(path.cmds.iter().filter(|c| matches!(c, Cmd::Z)).count(), 2);
    }

    #[test]
    fn a_ring_on_the_first_mark_remains_closed_at_an_axis() {
        let fixture = |name: &str| {
            std::fs::read_to_string(format!(
                "{}/../../richview-chart/tests/fixtures/types/{name}.json",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap()
        };
        let link = Rgba([0.1, 0.2, 0.9, 1.0]);
        for (kind, contoured) in [
            ("band", false),
            ("ecdf", false),
            ("strip", true),
            ("dotplot", true),
            ("candlestick", true),
            ("combo", true),
        ] {
            let g = chart(&fixture(kind));
            let panel = &g.panels[0];
            let t = Target {
                panel: 0,
                observation: 0,
            };
            let pointer = panel.observations[0].dots.first().map(|d| d.at);
            let outlines = outlines(kind, &g, t, pointer, contoured);
            let rings = mark_ring(&ring_lines(&outlines), link, 1.0);
            assert!(!rings.is_empty(), "{kind}: the first mark has a ring");
            for ring in rings {
                let Item::Area { path, .. } = ring else {
                    panic!("{kind}: a ring has filled paths");
                };
                assert!(
                    matches!(path.cmds.last(), Some(Cmd::Z)),
                    "{kind}: a ring is open"
                );
            }
        }
    }

    #[test]
    fn the_mark_ring_follows_each_family_two_pixels_past_its_contour() {
        let fixture = |name: &str| {
            std::fs::read_to_string(format!(
                "{}/../../richview-chart/tests/fixtures/types/{name}.json",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap()
        };
        ring_clears("combo", &fixture("combo"), true, 1);
        ring_clears("rose", &fixture("rose"), true, 1);
        ring_clears("dumbbell", &fixture("dumbbell"), true, 2);
        ring_clears("ohlc", &fixture("ohlc"), true, 1);
        ring_clears("candlestick", &fixture("candlestick"), true, 1);
        ring_clears("lollipop", &fixture("lollipop"), true, 1);
        ring_clears("treemap", &fixture("treemap"), true, 1);
        ring_clears("calendar", &fixture("calendar"), false, 1);
        ring_clears("calendar", &fixture("calendar"), true, 1);
        ring_clears("hexbin", &fixture("hexbin"), true, 1);
        ring_clears("waffle", &fixture("waffle"), true, 1);
        ring_clears("network", &fixture("network"), true, 1);
        ring_clears("parallel", &fixture("parallel"), true, 1);
        ring_clears("map", &fixture("map"), true, 1);
        ring_clears("sankey", &fixture("sankey"), true, 1);
        ring_clears("alluvial", &fixture("alluvial"), true, 1);
        ring_clears("violin", &fixture("violin"), true, 1);
        ring_clears("sunburst", &fixture("sunburst"), true, 1);
        ring_clears("funnel", &fixture("funnel"), true, 1);
        ring_clears("gantt", &fixture("gantt"), true, 1);
        ring_clears("bar3d", &fixture("bar3d"), true, 1);
        ring_clears("surface", &fixture("surface"), true, 1);
        ring_clears("contour", &fixture("contour"), false, 1);
        let band = chart(&fixture("band"));
        let observations = &band.panels[0].observations;
        let dots = observations[observations.len() / 2].dots.len();
        ring_clears("band", &fixture("band"), false, dots.max(1));
    }

    #[test]
    fn chord_keeps_the_selected_group_and_dims_the_others() {
        let g = chart(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../richview-chart/tests/fixtures/types/chord.json"
        )));
        let panel = &g.panels[0];
        let target = Target {
            panel: 0,
            observation: 0,
        };
        let selected = panel.observations[0].series.unwrap();
        let (focus, floor) =
            select("chord", &g, None, Some(target)).expect("chord emphasizes its group");
        assert_eq!(floor, 0.2);
        let marks: Vec<_> = panel
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    Item::Area {
                        role: Role::Mark,
                        series: Some(_),
                        ..
                    }
                )
            })
            .collect();
        assert!(marks.iter().any(|item| item.series() == Some(selected)));
        assert!(marks.iter().any(|item| item.series() != Some(selected)));
        for item in marks {
            assert_eq!(focus.keeps(0, item), item.series() == Some(selected));
        }
    }
}
