//! The geometry model as GPU-ready triangles (ARC-18). Pure Rust, so the tessellation is tested
//! natively; the WebGL module only uploads and draws what this produces. It adds no geometry of
//! its own: every vertex comes from an item of the model.
//!
//! Vertex layout, ten floats: position (2), colour (4, straight alpha), motion anchor (2), motion
//! kind (1) and motion delay in ms (1).
//! Analytic circles flag the kind and use the delay slot for their radius.

use richview_chart::geometry::{
    Cap, Cmd, Fill, Geometry, Hatch, Item, Panel, Path, Rect, Rgba, Role, Shape, Stroke,
};
use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::sync::OnceLock;

pub const FLOATS: usize = 10;
// Circle encoding: kind retains its motion below this flag; delay holds the radius.
// Only motions that ignore delay are eligible. Keep the existing vertex/upload contract.
pub(crate) const ANALYTIC_CIRCLE: f32 = 16.0;

/// How a vertex enters (FEA-CH-51); the shader turns kind, anchor and delay into a position and
/// an alpha at time `t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Motion {
    Still = 0,
    /// Grow along y towards the anchor's y (vertical bars).
    GrowY = 1,
    /// Revealed left to right by the sweep (lines, areas).
    Sweep = 2,
    /// Revealed clockwise around the anchor (pie slices).
    Fan = 3,
    /// Fade and settle in place (points, dense marks).
    Settle = 4,
    /// Grow along x towards the anchor's x (horizontal bars).
    GrowX = 5,
    /// Labels and annotations: fade in as the last marks land.
    Late = 6,
    /// Grow from the anchor in both axes, after the delay (tiles and cells without a baseline).
    Scale = 7,
    /// Fade in while growing from half size about the anchor (points).
    Pop = 8,
    /// Revealed by the sweep, fading in over the band behind its front (filled areas, flows).
    Wash = 9,
}

impl Motion {
    /// Whether an analytic circle can carry this motion: its delay slot holds the radius, so
    /// only motions that need no delay qualify.
    fn analytic(self, delay: f32) -> bool {
        match self {
            Motion::Still | Motion::Sweep | Motion::Settle => true,
            Motion::Pop => delay == 0.0,
            _ => false,
        }
    }
}

/// One draw step, in paint order.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Solid triangles: a vertex range.
    Tris {
        first: u32,
        count: u32,
        clip: Option<Rect>,
    },
    /// A polygon of any shape by stencil: the fan marks covered pixels, the cover paints them.
    Stencil {
        fan: (u32, u32),
        cover: (u32, u32),
        hatch: Option<(Hatch, Rgba, f32)>,
        clip: Option<Rect>,
    },
    /// One text item of the panel or figure, by index into [`Mesh::texts`].
    Text { index: u32, clip: Option<Rect> },
}

/// A text item to rasterise: the model's text with where its quad goes.
#[derive(Debug, Clone, PartialEq)]
pub struct TextQuad {
    pub text: richview_chart::geometry::Text,
    /// Alpha after mark emphasis, before hover-label occlusion.
    pub base_alpha: f32,
    pub motion: Motion,
    pub delay: f32,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<f32>,
    pub ops: Vec<Op>,
    pub texts: Vec<TextQuad>,
}

/// What stays at full strength while the rest of a chart steps back (FEA-CH-52).
#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    /// One series: marks of every other series dim.
    Series(usize),
    /// Every series listed by the selected observation stays at full strength.
    SeriesSet(Vec<usize>),
    /// The series of a hovered mark: marks of every other series dim, except marks the
    /// hovered observation itself owns.
    SeriesOf {
        series: usize,
        panel: usize,
        observation: usize,
    },
    /// Marks of one panel by observation: its other owned marks dim. Marks that own no
    /// observation, labels and every other panel stay.
    Marks { panel: usize, keep: Vec<usize> },
}

impl Focus {
    pub(crate) fn keeps(&self, panel: usize, item: &Item) -> bool {
        match self {
            Focus::Series(s) => item.series().is_none_or(|k| k == *s),
            Focus::SeriesSet(series) => item.series().is_none_or(|k| series.contains(&k)),
            Focus::SeriesOf {
                series,
                panel: p,
                observation,
            } => {
                item.series().is_none_or(|k| k == *series)
                    || (*p == panel && owner(item) == Some(*observation))
            }
            Focus::Marks { panel: p, keep } => {
                *p != panel || owner(item).is_none_or(|k| keep.contains(&k))
            }
        }
    }
}

/// The observation that owns a mark; labels and chrome have none.
fn owner(item: &Item) -> Option<usize> {
    match item {
        Item::Rect { obs, role, .. }
        | Item::Line { obs, role, .. }
        | Item::Area { obs, role, .. }
        | Item::Point { obs, role, .. }
            if *role == Role::Mark =>
        {
            *obs
        }
        _ => None,
    }
}

/// What the mesh draws beyond the model's settled items: what the reader emphasises and how
/// far the rest has stepped back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// What the reader emphasises; the rest draws at `floor` (FEA-CH-52).
    pub emphasis: Option<Focus>,
    /// How far the rest has dimmed: 0 at full strength, 1 at `floor`, so emphasis can fade in
    /// and out.
    pub dim: f32,
    /// The share of its alpha a fully dimmed mark keeps: 20%, 35% for point marks, and 10%
    /// for a flow's ribbons.
    pub floor: f32,
    /// The general floor, also used for flow nodes.
    pub node_floor: f32,
    /// How far the emphasised marks have come back from the dimmed rest, 0 to 1, when the
    /// emphasis moved to them from other marks.
    pub arrive: f32,
    /// The marks emphasised before and how much of their strength they still hold, 1 to 0.
    pub leaving: Option<(Focus, f32)>,
}

impl State {
    /// `series` emphasised, the others fully dimmed.
    pub fn emphasis(series: Option<usize>) -> Self {
        Self {
            emphasis: series.map(Focus::Series),
            dim: 1.0,
            floor: DIM_FLOOR,
            node_floor: DIM_FLOOR,
            arrive: 1.0,
            leaving: None,
        }
    }

    /// How far the emphasis keeps `item` at full strength now, 0 to 1: what has arrived of the
    /// marks emphasised now, or what the marks emphasised before still hold.
    fn light_of(&self, panel: usize, item: &Item) -> f32 {
        let Some(focus) = &self.emphasis else {
            return 0.0;
        };
        let mut light: f32 = if focus.keeps(panel, item) {
            self.arrive
        } else {
            0.0
        };
        if let Some((before, held)) = &self.leaving
            && before.keeps(panel, item)
        {
            light = light.max(*held);
        }
        light.clamp(0.0, 1.0)
    }

    /// The share of its alpha `item` loses now, 0 to 1.
    fn dim_of(&self, panel: usize, item: &Item) -> f32 {
        if self.emphasis.is_none() {
            return 0.0;
        }
        // A flow's nodes keep the general floor, so the diagram's skeleton stays readable
        // while its unconnected ribbons go further back.
        let floor = match item {
            Item::Rect { .. } => self.floor.max(self.node_floor),
            _ => self.floor,
        };
        (1.0 - floor) * self.dim * (1.0 - self.light_of(panel, item))
    }
}

/// The observations a flow's `observation` connects, itself included. A ribbon keeps the two
/// nodes it joins; a node keeps its ribbons and the nodes they reach. The layout lists nodes
/// as rectangles and ribbons as areas that start on their source's right edge and end on
/// their target's left edge.
pub fn connected(panel: &Panel, observation: usize) -> Vec<usize> {
    let mut nodes: Vec<(usize, Rect)> = Vec::new();
    let mut ribbons: Vec<(usize, [f64; 2], [f64; 2])> = Vec::new();
    for item in &panel.items {
        match item {
            Item::Rect {
                rect,
                role: Role::Mark,
                obs: Some(k),
                ..
            } => nodes.push((*k, *rect)),
            Item::Area {
                path,
                role: Role::Mark,
                obs: Some(k),
                ..
            } => {
                // The upper edge runs from the source to the target.
                let start = path.cmds.iter().find_map(|c| match c {
                    Cmd::M { p } => Some(*p),
                    _ => None,
                });
                let end = path.cmds.iter().find_map(|c| match c {
                    Cmd::C { p, .. } => Some(*p),
                    _ => None,
                });
                if let (Some(start), Some(end)) = (start, end) {
                    ribbons.push((*k, start, end));
                }
            }
            _ => {}
        }
    }
    let node_at = |[x, y]: [f64; 2], right: bool| {
        nodes.iter().find_map(|(k, r)| {
            let edge = if right { r.right() } else { r.x };
            ((edge - x).abs() < 0.75 && y >= r.y - 0.75 && y <= r.bottom() + 0.75).then_some(*k)
        })
    };
    let ends: Vec<(usize, Option<usize>, Option<usize>)> = ribbons
        .iter()
        .map(|(k, start, end)| (*k, node_at(*start, true), node_at(*end, false)))
        .collect();
    let mut keep = vec![observation];
    for (ribbon, source, target) in ends {
        let joins = [source, target].contains(&Some(observation));
        if ribbon == observation || joins {
            keep.push(ribbon);
            keep.extend(source);
            keep.extend(target);
        }
    }
    keep.sort_unstable();
    keep.dedup();
    keep
}

/// The series whose filled layer contains `point`, for charts of stacked layers.
pub fn layer_at(panel: &Panel, [x, y]: [f64; 2]) -> Option<usize> {
    panel.items.iter().find_map(|item| {
        let Item::Area {
            path,
            role: Role::Mark,
            series: Some(series),
            ..
        } = item
        else {
            return None;
        };
        // Even-odd crossings of a ray to the right of the point.
        let mut inside = false;
        for ring in path.flatten(0.5) {
            for (k, a) in ring.iter().enumerate() {
                let b = ring[(k + 1) % ring.len()];
                if (a[1] > y) != (b[1] > y) && x < a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
                {
                    inside = !inside;
                }
            }
        }
        inside.then_some(*series)
    })
}

/// What a dimmed series, slice or bar keeps of its alpha.
pub const DIM_FLOOR: f32 = 0.2;
/// What a dimmed point mark keeps: a cloud of points loses its shape below this.
pub const POINT_DIM_FLOOR: f32 = 0.35;
/// What an unconnected ribbon keeps in a flow chart; its nodes keep [`DIM_FLOOR`].
pub const FLOW_DIM_FLOOR: f32 = 0.1;

/// The same floor on light pages, raised on dark pages to retain the chart's shape.
pub fn page_floor(floor: f32, page: &str) -> f32 {
    if richview_chart::color::parse(page).is_some_and(|color| color.l < 0.5) {
        floor * 1.6
    } else {
        floor
    }
}

/// A mark that can enclose a label, with its flattened fill kept for this mesh build.
enum LabelShape {
    Rect(Rect),
    Circle { center: [f64; 2], radius: f64 },
    Area(Vec<Vec<[f64; 2]>>),
}

impl LabelShape {
    fn contains(&self, bounds: Rect) -> bool {
        let corners = [
            [bounds.x, bounds.y],
            [bounds.right(), bounds.y],
            [bounds.right(), bounds.bottom()],
            [bounds.x, bounds.bottom()],
        ];
        match self {
            Self::Rect(rect) => bounds.inside(rect, 0.0),
            Self::Circle { center, radius } => corners
                .iter()
                .all(|p| (p[0] - center[0]).hypot(p[1] - center[1]) <= *radius),
            Self::Area(rings) => corners.iter().all(|&[x, y]| {
                let mut inside = false;
                for ring in rings {
                    for (index, a) in ring.iter().enumerate() {
                        let b = ring[(index + 1) % ring.len()];
                        if (a[1] > y) != (b[1] > y)
                            && x < a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1])
                        {
                            inside = !inside;
                        }
                    }
                }
                inside
            }),
        }
    }
}

struct Builder {
    mesh: Mesh,
    /// Pixels per CSS pixel: curves flatten finer on dense screens.
    scale: f64,
    open: Option<(u32, Option<Rect>)>,
    analytic_circles: bool,
}

#[derive(Clone, Copy)]
struct Paint {
    color: [f32; 4],
    anchor: [f64; 2],
    motion: Motion,
    delay: f32,
}

// Round-join tessellation uses only these integer segment counts. Cache the exact
// trigonometric samples, including the final TAU endpoint; centres and radii stay fresh.
fn unit_circle(steps: usize) -> &'static [[f64; 2]] {
    static CIRCLES: [OnceLock<Vec<[f64; 2]>>; 49] = [const { OnceLock::new() }; 49];
    CIRCLES[steps].get_or_init(|| {
        (0..=steps)
            .map(|i| {
                let angle = i as f64 / steps as f64 * TAU;
                [angle.cos(), angle.sin()]
            })
            .collect()
    })
}

impl Builder {
    fn vertex(&mut self, p: [f64; 2], paint: &Paint) {
        let [r, g, b, a] = paint.color;
        self.mesh.vertices.extend_from_slice(&[
            p[0] as f32,
            p[1] as f32,
            r,
            g,
            b,
            a,
            paint.anchor[0] as f32,
            paint.anchor[1] as f32,
            paint.motion as u8 as f32,
            paint.delay,
        ]);
    }
    fn count(&self) -> u32 {
        (self.mesh.vertices.len() / FLOATS) as u32
    }
    /// Start or continue a solid run with this clip.
    fn tris(&mut self, clip: Option<Rect>) {
        match self.open {
            Some((_, c)) if c == clip => {}
            _ => {
                self.close();
                self.open = Some((self.count(), clip));
            }
        }
    }
    fn close(&mut self) {
        if let Some((first, clip)) = self.open.take() {
            let count = self.count() - first;
            if count > 0 {
                self.mesh.ops.push(Op::Tris { first, count, clip });
            }
        }
    }
    fn tri(&mut self, a: [f64; 2], b: [f64; 2], c: [f64; 2], paint: &Paint) {
        self.vertex(a, paint);
        self.vertex(b, paint);
        self.vertex(c, paint);
    }
    fn quad(&mut self, a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2], paint: &Paint) {
        self.tri(a, b, c, paint);
        self.tri(a, c, d, paint);
    }
    fn circle_quad(&mut self, center: [f64; 2], radius: f64, paint: &Paint) {
        let reach = radius + 0.5 / self.scale.max(0.5);
        let [x, y] = center;
        let first = self.mesh.vertices.len();
        self.quad(
            [x - reach, y - reach],
            [x + reach, y - reach],
            [x + reach, y + reach],
            [x - reach, y + reach],
            paint,
        );
        for vertex in self.mesh.vertices[first..].as_chunks_mut::<FLOATS>().0 {
            vertex[8] += ANALYTIC_CIRCLE;
            vertex[9] = radius as f32;
        }
    }
    fn tolerance(&self) -> f64 {
        0.25 / self.scale.max(0.5)
    }

    /// A stroked polyline with round joins and the stroke's caps and dashes.
    fn stroke(&mut self, line: &[[f64; 2]], stroke: &Stroke, paint: &Paint) {
        let pieces = if stroke.dash.is_empty() {
            vec![line.to_vec()]
        } else {
            dash(line, &stroke.dash)
        };
        let half = stroke.width / 2.0;
        for piece in pieces {
            if piece.len() < 2 {
                continue;
            }
            for w in piece.windows(2) {
                let (a, b) = (w[0], w[1]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let len = dx.hypot(dy);
                if len < 1e-9 {
                    continue;
                }
                let (nx, ny) = (-dy / len * half, dx / len * half);
                self.quad(
                    [a[0] + nx, a[1] + ny],
                    [b[0] + nx, b[1] + ny],
                    [b[0] - nx, b[1] - ny],
                    [a[0] - nx, a[1] - ny],
                    paint,
                );
            }
            // Round joins at interior vertices keep wide strokes closed at bends.
            if half > 0.6 {
                for p in &piece[1..piece.len() - 1] {
                    self.disc(*p, half, paint);
                }
            }
            let (first, last) = (piece[0], piece[piece.len() - 1]);
            match stroke.cap {
                Cap::Butt => {}
                Cap::Round => {
                    self.disc(first, half, paint);
                    self.disc(last, half, paint);
                }
                Cap::Square => {
                    let ext = |p: [f64; 2], q: [f64; 2]| {
                        let (dx, dy) = (p[0] - q[0], p[1] - q[1]);
                        let l = dx.hypot(dy).max(1e-9);
                        [p[0] + dx / l * half, p[1] + dy / l * half]
                    };
                    let a = ext(first, piece[1]);
                    let b = ext(last, piece[piece.len() - 2]);
                    let caps = |from: [f64; 2], to: [f64; 2], this: &mut Builder| {
                        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
                        let l = dx.hypot(dy).max(1e-9);
                        let (nx, ny) = (-dy / l * half, dx / l * half);
                        this.quad(
                            [from[0] + nx, from[1] + ny],
                            [to[0] + nx, to[1] + ny],
                            [to[0] - nx, to[1] - ny],
                            [from[0] - nx, from[1] - ny],
                            paint,
                        );
                    };
                    caps(a, first, self);
                    caps(last, b, self);
                }
            }
        }
    }

    fn disc(&mut self, c: [f64; 2], r: f64, paint: &Paint) {
        if self.analytic_circles && r.is_finite() && r >= 1.0 && paint.motion.analytic(paint.delay)
        {
            self.circle_quad(
                c,
                r,
                &Paint {
                    anchor: c,
                    ..*paint
                },
            );
            return;
        }
        let steps = ((r * self.scale * 2.0).sqrt() * 4.0).clamp(8.0, 48.0) as usize;
        for pair in unit_circle(steps).windows(2) {
            self.tri(
                c,
                [c[0] + r * pair[0][0], c[1] + r * pair[0][1]],
                [c[0] + r * pair[1][0], c[1] + r * pair[1][1]],
                paint,
            );
        }
    }

    /// A polygon by stencil: fan from its first point, then a cover quad over its bounds.
    fn stencil(&mut self, rings: &[Vec<[f64; 2]>], fill: &Fill, paint: &Paint, clip: Option<Rect>) {
        self.close();
        let first = self.count();
        let mut bounds: Option<Rect> = None;
        for ring in rings {
            if ring.len() < 3 {
                continue;
            }
            let o = ring[0];
            for w in ring[1..].windows(2) {
                self.tri(o, w[0], w[1], paint);
            }
            for p in ring {
                let r = Rect::new(p[0], p[1], 0.0, 0.0);
                bounds = Some(bounds.map_or(r, |b| b.union(&r)));
            }
        }
        let fan = (first, self.count() - first);
        let Some(b) = bounds else { return };
        let cover_first = self.count();
        let b = b.inflate(1.0);
        self.quad(
            [b.x, b.y],
            [b.right(), b.y],
            [b.right(), b.bottom()],
            [b.x, b.bottom()],
            paint,
        );
        let hatch = fill.hatch.map(|h| {
            (
                h,
                fill.hatch_ink.unwrap_or(Rgba([1.0, 1.0, 1.0, 1.0])),
                fill.hatch_scale as f32,
            )
        });
        self.mesh.ops.push(Op::Stencil {
            fan,
            cover: (cover_first, 6),
            hatch,
            clip,
        });
    }
}

/// Split a polyline into its dashes.
pub fn dash(line: &[[f64; 2]], pattern: &[f64]) -> Vec<Vec<[f64; 2]>> {
    let total: f64 = pattern.iter().sum();
    if total <= 0.0 || pattern.iter().any(|d| *d < 0.0) {
        return vec![line.to_vec()];
    }
    let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
    let (mut index, mut left, mut on) = (0usize, pattern[0], true);
    let mut current: Vec<[f64; 2]> = line.first().map(|p| vec![*p]).unwrap_or_default();
    for w in line.windows(2) {
        let (mut a, b) = (w[0], w[1]);
        let mut len = (b[0] - a[0]).hypot(b[1] - a[1]);
        while len > 1e-12 {
            let step = left.min(len);
            let t = step / len;
            let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            if on {
                current.push(p);
            }
            left -= step;
            len -= step;
            a = p;
            if left <= 1e-12 {
                if on && current.len() > 1 {
                    out.push(std::mem::take(&mut current));
                }
                current.clear();
                index = (index + 1) % pattern.len();
                left = pattern[index];
                on = !on;
                if on {
                    current.push(a);
                }
            }
        }
    }
    if on && current.len() > 1 {
        out.push(current);
    }
    out
}

/// The corners of a rectangle with per-corner radii, as one ring.
pub fn rounded_ring(rect: Rect, radius: [f64; 4], tolerance: f64) -> Vec<[f64; 2]> {
    let [tl, tr, br, bl] = radius.map(|v| v.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0));
    let mut ring = Vec::new();
    let mut corner = |c: [f64; 2], r: f64, from: f64| {
        if r <= 0.0 {
            ring.push(c);
            return;
        }
        let steps = ((r / tolerance).sqrt().ceil() as usize).clamp(2, 16);
        for i in 0..=steps {
            let a = from + FRAC_PI_2 * i as f64 / steps as f64;
            ring.push([c[0] + r * a.cos(), c[1] + r * a.sin()]);
        }
    };
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    corner(if tl > 0.0 { [x + tl, y + tl] } else { [x, y] }, tl, PI);
    corner(
        if tr > 0.0 {
            [x + w - tr, y + tr]
        } else {
            [x + w, y]
        },
        tr,
        -FRAC_PI_2,
    );
    corner(
        if br > 0.0 {
            [x + w - br, y + h - br]
        } else {
            [x + w, y + h]
        },
        br,
        0.0,
    );
    corner(
        if bl > 0.0 {
            [x + bl, y + h - bl]
        } else {
            [x, y + h]
        },
        bl,
        FRAC_PI_2,
    );
    ring
}

/// The outline of a marker shape, as rings (filled) and strokes (plus and cross).
pub(crate) fn shape_outline(
    shape: Shape,
    c: [f64; 2],
    r: f64,
) -> (Vec<[f64; 2]>, Vec<[[f64; 2]; 2]>) {
    let [x, y] = c;
    match shape {
        Shape::Circle => (
            unit_circle(32)[..32]
                .iter()
                .map(|p| [x + r * p[0], y + r * p[1]])
                .collect(),
            vec![],
        ),
        Shape::Square => (
            vec![
                [x - r * 0.85, y - r * 0.85],
                [x + r * 0.85, y - r * 0.85],
                [x + r * 0.85, y + r * 0.85],
                [x - r * 0.85, y + r * 0.85],
            ],
            vec![],
        ),
        Shape::Triangle => (
            vec![
                [x, y - r],
                [x + r * 0.95, y + r * 0.7],
                [x - r * 0.95, y + r * 0.7],
            ],
            vec![],
        ),
        Shape::Diamond => (vec![[x, y - r], [x + r, y], [x, y + r], [x - r, y]], vec![]),
        Shape::Plus => (
            vec![],
            vec![[[x, y - r], [x, y + r]], [[x - r, y], [x + r, y]]],
        ),
        Shape::Cross => {
            let k = r * 0.75;
            (
                vec![],
                vec![
                    [[x - k, y - k], [x + k, y + k]],
                    [[x + k, y - k], [x - k, y + k]],
                ],
            )
        }
    }
}

fn clipped(role: Role) -> bool {
    matches!(role, Role::Mark | Role::Band | Role::Rug | Role::Missing)
}

/// `c` without the share `dim` of its alpha (0 to 1).
fn rgba(c: Rgba, dim: f32) -> [f32; 4] {
    let [r, g, b, a] = c.0;
    [r, g, b, a * (1.0 - dim)]
}

/// Motion per item: what entrance it takes and where it grows from (FEA-CH-51).
fn motion_of(
    g: &Geometry,
    item: &Item,
    baseline: Option<Baseline>,
    tile: Option<(Motion, [f64; 2], f32)>,
    index: usize,
    marks: usize,
) -> (Motion, [f64; 2], f32) {
    use richview_chart::geometry::Entrance;
    let Some(entrance) = g.entrance else {
        return (Motion::Still, [0.0; 2], 0.0);
    };
    let role = item.role();
    if matches!(
        role,
        Role::Label | Role::Annotation | Role::Leader | Role::Missing
    ) {
        return (Motion::Late, [0.0; 2], 0.0);
    }
    if !matches!(role, Role::Mark | Role::Band | Role::Rug) {
        return (Motion::Still, [0.0; 2], 0.0);
    }
    let point = matches!(item, Item::Point { .. });
    let pops = pops(g, item);
    match entrance {
        // Strokes and markers end at the sweep's front; fills fade in behind it.
        Entrance::Sweep if point || matches!(item, Item::Line { .. }) => {
            (Motion::Sweep, [0.0; 2], 0.0)
        }
        Entrance::Sweep => (Motion::Wash, [0.0; 2], 0.0),
        // Filled points grow in from half their radius; a dense chart fades as a whole.
        Entrance::Settle if pops => (Motion::Pop, [0.0; 2], 0.0),
        Entrance::Fade if pops && marks < DENSE_MARKS => (Motion::Pop, [0.0; 2], 0.0),
        Entrance::Fade | Entrance::Settle => (Motion::Settle, [0.0; 2], 0.0),
        Entrance::Fan => {
            let center = match item {
                Item::Area { .. } => g
                    .panels
                    .iter()
                    .flat_map(|p| &p.observations)
                    .find_map(|o| o.wedge.map(|w| w.center))
                    .unwrap_or([0.0; 2]),
                _ => [0.0; 2],
            };
            (Motion::Fan, center, 0.0)
        }
        Entrance::Grow => match baseline {
            // Bars grow from the panel's zero line, staggered in reading order.
            Some(Baseline::Horizontal(y)) => {
                (Motion::GrowY, [0.0, y], g.timing.delay(index, marks) as f32)
            }
            Some(Baseline::Vertical(x)) => {
                (Motion::GrowX, [x, 0.0], g.timing.delay(index, marks) as f32)
            }
            // Tiles and cells without a baseline: see `tiles`.
            None => tile.unwrap_or((Motion::Settle, [0.0; 2], 0.0)),
        },
    }
}

/// Whether a point grows in from half its radius: one filled with a colour of its own. A hollow
/// point (no fill, or one filled with the chart's background inside a ring) fades in place,
/// because its ring at half size would cross its own hollow, where the settled chart shows the
/// page.
fn pops(g: &Geometry, item: &Item) -> bool {
    let Item::Point { fill: Some(f), .. } = item else {
        return false;
    };
    let ([r, gr, b, _], [pr, pg, pb, _]) = (f.color.0, g.background.0);
    f.color.0[3] > 0.0 && ((r - pr).abs() + (gr - pg).abs() + (b - pb).abs()) > 0.06
}

/// How tiles and cells without a baseline enter: each grows from its own centre, or from the
/// centre of the mark that contains it, in a wave from the first tile at the plot's top left to
/// the last, spread over [`wave_steps`] steps of the style's stagger. A centred stack such as a
/// funnel widens from its shared axis instead, so its stages stay joined; points grow from half
/// their radius.
fn tiles(g: &Geometry, items: &[Item], plot: Option<Rect>) -> Vec<Option<(Motion, [f64; 2], f32)>> {
    let groups = containers(items);
    let centres: Vec<Option<[f64; 2]>> = items
        .iter()
        .zip(&groups)
        .map(|(item, group)| {
            if !matches!(item.role(), Role::Mark | Role::Band | Role::Rug) {
                return None;
            }
            let b = group.or_else(|| item.bounds())?;
            Some([b.x + b.w / 2.0, b.y + b.h / 2.0])
        })
        .collect();
    let along = |[x, y]: [f64; 2]| {
        plot.map_or(0.0, |p| (x - p.x) / p.w.max(1.0) + (y - p.y) / p.h.max(1.0))
    };
    let (first, last) = centres
        .iter()
        .flatten()
        .map(|c| along(*c))
        .fold((f64::MAX, f64::MIN), |(a, b), s| (a.min(s), b.max(s)));
    let spread = g.timing.spread(wave_steps(items));
    let axes: Vec<f64> = centres
        .iter()
        .zip(items)
        .filter(|(_, item)| !matches!(item, Item::Point { .. }))
        .filter_map(|(c, _)| c.map(|c| c[0]))
        .collect();
    let centred = axes.len() >= 2 && axes.iter().all(|x| (x - axes[0]).abs() < 1.0);
    items
        .iter()
        .zip(&centres)
        .map(|(item, centre)| {
            let centre = (*centre)?;
            let share = if last > first {
                (along(centre) - first) / (last - first)
            } else {
                0.0
            };
            let delay = (spread * share) as f32;
            Some(match item {
                Item::Point { .. } if pops(g, item) => (Motion::Pop, centre, delay),
                Item::Point { .. } => (Motion::Settle, centre, delay),
                _ if centred => (Motion::GrowX, [centre[0], 0.0], delay),
                _ => (Motion::Scale, centre, delay),
            })
        })
        .collect()
}

/// The steps of a tile wave: the distinct columns plus the distinct rows of the panel's filled
/// marks, less one, so two boxes start a step apart rather than the whole cap apart.
fn wave_steps(items: &[Item]) -> usize {
    let mut columns = std::collections::BTreeSet::new();
    let mut rows = std::collections::BTreeSet::new();
    for item in items {
        if item.role() != Role::Mark || !matches!(item, Item::Rect { .. } | Item::Area { .. }) {
            continue;
        }
        if let Some(b) = item.bounds() {
            columns.insert(((b.x + b.w / 2.0) * 2.0).round() as i64);
            rows.insert(((b.y + b.h / 2.0) * 2.0).round() as i64);
        }
    }
    (columns.len() + rows.len()).saturating_sub(1).max(1)
}

/// How many marks an entrance staggers across, for the runtime clock: the most in any panel,
/// counting a panel's wave steps when its marks grow as tiles (see `tiles`).
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn stagger_count(g: &Geometry) -> usize {
    g.panels
        .iter()
        .map(|p| {
            let marks = p.items.iter().filter(|i| i.role() == Role::Mark).count();
            let tiled = g.entrance == Some(richview_chart::geometry::Entrance::Grow)
                && baseline_of(&p.items).is_none()
                && marks < DENSE_MARKS;
            if tiled { wave_steps(&p.items) } else { marks }
        })
        .max()
        .unwrap_or(0)
}

/// The box of the outermost filled mark that contains each item, so a group and the tiles
/// inside it grow from one centre at one time instead of drifting apart mid-entrance.
fn containers(items: &[Item]) -> Vec<Option<Rect>> {
    let boxes: Vec<Option<Rect>> = items
        .iter()
        .map(|i| match i {
            Item::Rect { .. } | Item::Area { .. } if i.role() == Role::Mark => i.bounds(),
            _ => None,
        })
        .collect();
    items
        .iter()
        .enumerate()
        .map(|(n, item)| {
            let own = item.bounds()?;
            boxes
                .iter()
                .enumerate()
                .filter_map(|(m, b)| {
                    b.filter(|b| m != n && b.w * b.h > own.w * own.h && own.inside(b, 0.5))
                })
                .max_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)))
        })
        .collect()
}

/// From this many marks a chart enters as a whole (FEA-CH-51).
const DENSE_MARKS: usize = 2000;

#[derive(Debug, Clone, Copy)]
enum Baseline {
    Horizontal(f64),
    Vertical(f64),
}

fn baseline_of(items: &[Item]) -> Option<Baseline> {
    items.iter().find_map(|i| match i {
        Item::Line {
            path,
            role: Role::Baseline,
            ..
        } => {
            let pts = path.flatten(1.0);
            let line = pts.first()?;
            let (a, b) = (line.first()?, line.last()?);
            if (a[1] - b[1]).abs() < 0.5 {
                Some(Baseline::Horizontal(a[1]))
            } else {
                Some(Baseline::Vertical(a[0]))
            }
        }
        _ => None,
    })
}

#[allow(clippy::too_many_arguments)]
fn push_item(
    b: &mut Builder,
    item: &Item,
    clip: Option<Rect>,
    dim: f32,
    motion: Motion,
    anchor: [f64; 2],
    delay: f32,
    tol: f64,
) {
    match item {
        Item::Rect {
            rect,
            fill,
            stroke,
            radius,
            ..
        } => {
            let rect = rect.clamped();
            let paint = Paint {
                color: rgba(fill.color, dim),
                anchor,
                motion,
                delay,
            };
            let plain = radius.iter().all(|r| *r <= 0.0);
            if fill.hatch.is_none() && plain {
                b.tris(clip);
                b.quad(
                    [rect.x, rect.y],
                    [rect.right(), rect.y],
                    [rect.right(), rect.bottom()],
                    [rect.x, rect.bottom()],
                    &paint,
                );
            } else {
                let ring = rounded_ring(rect, *radius, tol);
                b.stencil(&[ring], fill, &paint, clip);
            }
            if let Some(s) = stroke {
                let ring = rounded_ring(rect, *radius, tol);
                let mut closed = ring.clone();
                closed.push(ring[0]);
                b.tris(clip);
                b.stroke(
                    &closed,
                    s,
                    &Paint {
                        color: rgba(s.color, dim),
                        ..paint
                    },
                );
            }
        }
        Item::Line { path, stroke, .. } => {
            let paint = Paint {
                color: rgba(stroke.color, dim),
                anchor,
                motion,
                delay,
            };
            b.tris(clip);
            for line in path.flatten(tol) {
                b.stroke(&line, stroke, &paint);
            }
        }
        Item::Area { path, fill, .. } => {
            let paint = Paint {
                color: rgba(fill.color, dim),
                anchor,
                motion,
                delay,
            };
            b.stencil(&path.flatten(tol), fill, &paint, clip);
        }
        Item::Point {
            center,
            radius,
            shape,
            fill,
            stroke,
            ..
        } => {
            if b.analytic_circles
                && *shape == Shape::Circle
                && radius.is_finite()
                && *radius >= 1.0
                && stroke.is_none()
                && (motion == Motion::Fan || motion.analytic(delay))
                && let Some(f) = fill
                && f.hatch.is_none()
            {
                b.tris(clip);
                b.circle_quad(
                    *center,
                    *radius,
                    &Paint {
                        color: rgba(f.color, dim),
                        anchor: *center,
                        motion,
                        delay,
                    },
                );
                return;
            }
            let (ring, bars) = shape_outline(*shape, *center, *radius);
            if let Some(f) = fill
                && ring.len() >= 3
            {
                let paint = Paint {
                    color: rgba(f.color, dim),
                    anchor: *center,
                    motion,
                    delay,
                };
                b.tris(clip);
                let o = ring[0];
                for w in ring[1..].windows(2) {
                    b.tri(o, w[0], w[1], &paint);
                }
            }
            if let Some(s) = stroke {
                let paint = Paint {
                    color: rgba(s.color, dim),
                    anchor: *center,
                    motion,
                    delay,
                };
                b.tris(clip);
                if ring.len() >= 3 {
                    let mut closed = ring.clone();
                    closed.push(ring[0]);
                    b.stroke(&closed, s, &paint);
                }
                for bar in bars {
                    b.stroke(&bar, s, &paint);
                }
            }
        }
        Item::Text(t) => {
            b.close();
            let text_motion = if motion == Motion::Late {
                Motion::Late
            } else {
                Motion::Still
            };
            let mut text = t.clone();
            text.color.0[3] *= 1.0 - dim;
            let base_alpha = text.color.0[3];
            b.mesh.texts.push(TextQuad {
                text,
                base_alpha,
                motion: text_motion,
                delay: 0.0,
            });
            b.mesh.ops.push(Op::Text {
                index: (b.mesh.texts.len() - 1) as u32,
                clip: None,
            });
        }
    }
}

/// Items drawn over a settled chart, such as the hover overlay, clipped to `clip`.
pub fn overlay(items: &[Item], clip: Option<Rect>, scale: f64) -> Mesh {
    overlay_layers([(items, clip)], scale)
}

/// Draw each feedback layer inside the plot it belongs to, in the given order.
pub(crate) fn overlay_layers<'a>(
    layers: impl IntoIterator<Item = (&'a [Item], Option<Rect>)>,
    scale: f64,
) -> Mesh {
    let mut b = Builder {
        mesh: Mesh::default(),
        scale,
        open: None,
        analytic_circles: false,
    };
    let tol = b.tolerance();
    for (items, clip) in layers {
        for item in items {
            push_item(&mut b, item, clip, 0.0, Motion::Still, [0.0; 2], 0.0, tol);
        }
    }
    b.close();
    b.mesh
}

/// Tessellate a chart at `scale` device pixels per CSS pixel.
pub fn build(g: &Geometry, state: &State, scale: f64) -> Mesh {
    build_with_analytic_circles(g, state, scale, true)
}

/// Build the host mesh and record each panel's safe grid-before-marks insertion point.
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn build_for_host(
    g: &Geometry,
    state: &State,
    scale: f64,
) -> (Mesh, Vec<Option<usize>>) {
    let mut splits = Vec::with_capacity(g.panels.len());
    let mesh = build_inner(g, state, scale, true, Some(&mut splits));
    (mesh, splits)
}

// Keep the fan available as a native regression reference, without a browser switch.
fn build_with_analytic_circles(
    g: &Geometry,
    state: &State,
    scale: f64,
    analytic_circles: bool,
) -> Mesh {
    build_inner(g, state, scale, analytic_circles, None)
}

fn build_inner(
    g: &Geometry,
    state: &State,
    scale: f64,
    analytic_circles: bool,
    mut splits: Option<&mut Vec<Option<usize>>>,
) -> Mesh {
    let mut b = Builder {
        mesh: Mesh::default(),
        scale,
        open: None,
        analytic_circles,
    };
    let tol = b.tolerance();
    for (panel_index, (panel_items, plot)) in g
        .panels
        .iter()
        .map(|p| (&p.items, Some(p.plot)))
        .chain(std::iter::once((&g.figure_items, None)))
        .enumerate()
    {
        // A grid after a data item cannot satisfy both sides of the insertion contract without
        // reordering the original drawing. Do not manufacture a boundary for that panel.
        let before_marks = if splits.is_some()
            && g.panels.get(panel_index).is_some_and(|panel| {
                panel.targeting == richview_chart::geometry::Targeting::NearestX
            }) {
            panel_items
                .iter()
                .position(|item| clipped(item.role()))
                .filter(|&first| {
                    !panel_items[first..]
                        .iter()
                        .any(|item| item.role() == Role::Grid)
                })
        } else {
            None
        };
        let mut split = None;
        let baseline = baseline_of(panel_items);
        let marks = panel_items
            .iter()
            .filter(|i| i.role() == Role::Mark)
            .count();
        let tiles = if g.entrance == Some(richview_chart::geometry::Entrance::Grow)
            && baseline.is_none()
            && marks < DENSE_MARKS
        {
            tiles(g, panel_items, plot)
        } else {
            vec![None; panel_items.len()]
        };
        let mut mark_index = 0usize;
        let mark_boxes: Vec<_> = if state.emphasis.is_some() && state.dim > 0.0 {
            let labels: Vec<_> = panel_items
                .iter()
                .filter_map(|item| match item {
                    Item::Text(text) => Some(text.bounds),
                    _ => None,
                })
                .collect();
            panel_items
                .iter()
                .filter(|item| item.role() == Role::Mark)
                .filter_map(|item| {
                    let bounds = item.bounds()?;
                    if !labels.iter().any(|label| label.inside(&bounds, 0.0)) {
                        return None;
                    }
                    let shape = match item {
                        Item::Rect { rect, .. } => LabelShape::Rect(*rect),
                        Item::Point { center, radius, .. } => LabelShape::Circle {
                            center: *center,
                            radius: *radius,
                        },
                        Item::Area { path, .. } => LabelShape::Area(path.flatten(0.5)),
                        _ => return None,
                    };
                    Some((bounds, shape, state.dim_of(panel_index, item)))
                })
                .collect()
        } else {
            Vec::new()
        };
        // A 3-D bar's rows are its series, and the rows nearer the viewer paint over the row the
        // reader emphasises. That row is painted a second time on top, as opaque as the others
        // have stepped back, so it shows in its own colour and nothing changes places: at rest
        // the drawing is the model's, and a bar of the row still hides the bars behind it
        // (FEA-CH-52).
        let over_rows = g.kind == "bar3d"
            && panel_index < g.panels.len()
            && state.emphasis.is_some()
            && state.dim > 0.0;
        let mut again = Vec::new();
        for (item_index, item) in panel_items.iter().enumerate() {
            if before_marks == Some(item_index) {
                b.close();
                split = Some(b.mesh.ops.len());
            }
            // Marks keep to the plot, grown by their own half-extent so points and strokes on its
            // edges draw whole.
            let clip = if clipped(item.role()) {
                let reach = match item {
                    Item::Point { radius, stroke, .. } => {
                        radius + stroke.as_ref().map_or(0.0, |s| s.width / 2.0)
                    }
                    Item::Line { stroke, .. } => stroke.width / 2.0,
                    _ => 0.0,
                };
                plot.map(|p| p.inflate(1.0 + reach))
            } else {
                None
            };
            let dim = if let Item::Text(text) = item {
                mark_boxes
                    .iter()
                    .filter(|(bounds, shape, _)| {
                        text.bounds.inside(bounds, 0.0) && shape.contains(text.bounds)
                    })
                    .min_by(|(a, _, _), (b, _, _)| (a.w * a.h).total_cmp(&(b.w * b.h)))
                    .map_or(0.0, |(_, _, dim)| *dim)
            } else {
                state.dim_of(panel_index, item)
            };
            let (motion, anchor, delay) =
                motion_of(g, item, baseline, tiles[item_index], mark_index, marks);
            if item.role() == Role::Mark {
                mark_index += 1;
            }
            push_item(&mut b, item, clip, dim, motion, anchor, delay, tol);
            if over_rows && item.series().is_some() && !matches!(item, Item::Text(_)) {
                let shown = state.dim * state.light_of(panel_index, item);
                if shown > 0.0 {
                    again.push((item, clip, 1.0 - shown, motion, anchor, delay));
                }
            }
        }
        for (item, clip, dim, motion, anchor, delay) in again {
            push_item(&mut b, item, clip, dim, motion, anchor, delay, tol);
        }
        if panel_index < g.panels.len()
            && let Some(splits) = splits.as_deref_mut()
        {
            splits.push(split);
        }
    }
    b.close();
    b.mesh
}

/// The path of a polyline, for overlays built from the model's own positions.
pub fn polyline(points: &[[f64; 2]]) -> Path {
    Path::polyline(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{ChartTheme, Options, layout};
    use serde_json::json;

    #[test]
    fn cached_circle_outlines_keep_the_original_vertices_bit_for_bit() {
        for centre in [[0.0, 0.0], [1e-14, -1e-14], [120.25, -80.33], [1e7, -1e7]] {
            for radius in [0.0, 0.125, 4.0, 16.0, 1e6] {
                let (ring, _) = shape_outline(Shape::Circle, centre, radius);
                for (i, actual) in ring.iter().enumerate() {
                    let angle = i as f64 / 32.0 * TAU;
                    let expected = [
                        centre[0] + radius * angle.cos(),
                        centre[1] + radius * angle.sin(),
                    ];
                    assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
                }
            }
        }
    }

    fn chart(spec: serde_json::Value, rows: &[serde_json::Value]) -> Geometry {
        layout(
            &ChartBlock::from_value(&spec).unwrap(),
            rows,
            &ChartTheme::default(),
            &Options {
                width: 640.0,
                ..Options::default()
            },
        )
    }

    #[test]
    fn dark_page_floors_keep_the_chart_visible() {
        for (page, factor) in [
            ("oklch(0.8 0 0)", 1.0),
            ("oklch(0.49 0 0)", 1.6),
            ("oklch(0.5 0 0)", 1.0),
        ] {
            for floor in [DIM_FLOOR, POINT_DIM_FLOOR, FLOW_DIM_FLOOR] {
                assert!((page_floor(floor, page) - floor * factor).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn only_text_inside_a_dimmed_mark_follows_its_opacity() {
        let mut g = chart(
            json!({"type":"bar", "title":"T", "encoding":{"x":"x","y":"y"}}),
            &[json!({"x":"A","y":1}), json!({"x":"B","y":2})],
        );
        let mut text = match g.panels[0]
            .items
            .iter()
            .find(|i| matches!(i, Item::Text(_)))
            .unwrap()
            .clone()
        {
            Item::Text(t) => t,
            _ => unreachable!(),
        };
        text.color = Rgba([0.0, 0.0, 0.0, 1.0]);
        text.bounds = Rect::new(12.0, 12.0, 4.0, 4.0);
        let mut outside = text.clone();
        outside.bounds = Rect::new(40.0, 40.0, 4.0, 4.0);
        let mark = Item::Rect {
            rect: Rect::new(10.0, 10.0, 10.0, 10.0),
            fill: Fill::solid(Rgba([1.0; 4])),
            stroke: None,
            radius: [0.0; 4],
            role: Role::Mark,
            series: Some(1),
            obs: Some(1),
        };
        g.panels[0].items = vec![mark, Item::Text(text), Item::Text(outside)];
        let mesh = build(&g, &State::emphasis(Some(0)), 1.0);
        assert!((mesh.texts[0].text.color.0[3] - 0.2).abs() < 1e-6);
        assert_eq!(mesh.texts[1].text.color.0[3], 1.0);
    }

    #[test]
    fn sankey_node_labels_outside_ribbons_keep_full_strength() {
        let input: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../richview-chart/tests/fixtures/types/sankey.json"
        )))
        .unwrap();
        let g = chart(input["spec"].clone(), input["rows"].as_array().unwrap());
        let panel = &g.panels[0];
        let selected = panel
            .observations
            .iter()
            .position(|o| o.readout.heading == "Wind → Power")
            .unwrap();
        let state = State {
            emphasis: Some(Focus::Marks {
                panel: 0,
                keep: connected(panel, selected),
            }),
            dim: 1.0,
            floor: FLOW_DIM_FLOOR,
            node_floor: DIM_FLOOR,
            arrive: 1.0,
            leaving: None,
        };
        let mesh = build(&g, &state, 1.0);
        for name in ["Power", "Heat"] {
            let text = mesh.texts.iter().find(|q| q.text.text == name).unwrap();
            let original = panel
                .items
                .iter()
                .find_map(|i| match i {
                    Item::Text(t) if t.text == name => Some(t.color.0[3]),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                text.text.color.0[3], original,
                "{name} must not dim with a ribbon whose bounding box contains it"
            );
        }
    }

    #[test]
    fn labels_follow_circle_and_area_fills_not_their_bounding_boxes() {
        let mut g = chart(
            json!({"type":"bar", "title":"T", "encoding":{"x":"x","y":"y"}}),
            &[json!({"x":"A","y":1}), json!({"x":"B","y":2})],
        );
        let text = match g.panels[0]
            .items
            .iter()
            .find(|i| matches!(i, Item::Text(_)))
            .unwrap()
            .clone()
        {
            Item::Text(t) => t,
            _ => unreachable!(),
        };
        let label = |bounds| {
            let mut t = text.clone();
            t.bounds = bounds;
            t.color = Rgba([0.0, 0.0, 0.0, 1.0]);
            Item::Text(t)
        };
        let circle = Item::Point {
            center: [10.0, 10.0],
            radius: 10.0,
            shape: Shape::Circle,
            fill: Some(Fill::solid(Rgba([1.0; 4]))),
            stroke: None,
            role: Role::Mark,
            series: Some(1),
            obs: Some(0),
        };
        let mut path = Path::polyline(&[[30.0, 0.0], [50.0, 0.0], [30.0, 20.0]]);
        path.close();
        let area = Item::Area {
            path,
            fill: Fill::solid(Rgba([1.0; 4])),
            role: Role::Mark,
            series: Some(1),
            obs: Some(1),
        };
        g.panels[0].items = vec![
            circle,
            area,
            label(Rect::new(9.0, 9.0, 2.0, 2.0)),
            label(Rect::new(0.5, 0.5, 1.0, 1.0)),
            label(Rect::new(31.0, 1.0, 2.0, 2.0)),
            label(Rect::new(47.0, 17.0, 2.0, 2.0)),
        ];
        let mesh = build(&g, &State::emphasis(Some(0)), 1.0);
        let alphas: Vec<_> = mesh.texts.iter().map(|q| q.text.color.0[3]).collect();
        assert!((alphas[0] - 0.2).abs() < 1e-6);
        assert_eq!(alphas[1], 1.0);
        assert!((alphas[2] - 0.2).abs() < 1e-6);
        assert_eq!(alphas[3], 1.0);
    }

    #[test]
    fn panel_boundaries_preserve_whole_chart_ops_and_precede_their_marks() {
        let mut g = chart(
            json!({"type":"line", "title":"T", "encoding":{"x":"x","y":"y"}}),
            &[json!({"x":0,"y":1}), json!({"x":1,"y":2})],
        );
        let text = g.panels[0]
            .items
            .iter()
            .find(|item| matches!(item, Item::Text(_)))
            .expect("chart has text")
            .clone();
        let grid = Item::Line {
            path: Path::polyline(&[[10.0, 20.0], [90.0, 20.0]]),
            stroke: Stroke::solid(Rgba([0.5, 0.5, 0.5, 1.0]), 1.0),
            role: Role::Grid,
            series: None,
            obs: None,
        };
        let mut shape = Path::polyline(&[[20.0, 30.0], [80.0, 30.0], [50.0, 80.0]]);
        shape.close();
        let mark = Item::Area {
            path: shape,
            fill: Fill::solid(Rgba([0.1, 0.2, 0.3, 0.5])),
            role: Role::Mark,
            series: Some(0),
            obs: None,
        };
        g.panels[0].items = vec![grid.clone(), text.clone(), mark, text];
        g.panels.push(g.panels[0].clone());
        let plain = build(&g, &State::default(), 1.0);
        let (with_splits, splits) = build_for_host(&g, &State::default(), 1.0);
        assert_eq!(with_splits.vertices, plain.vertices);
        assert_eq!(with_splits.texts, plain.texts);
        // Closing a triangle batch at the boundary can split an otherwise contiguous run;
        // coalesce those runs to compare the complete original paint order and clips.
        let coalesce = |ops: &[Op]| {
            let mut out: Vec<Op> = Vec::new();
            for op in ops {
                if let (
                    Some(Op::Tris { first, count, clip }),
                    Op::Tris {
                        first: next,
                        count: n,
                        clip: next_clip,
                    },
                ) = (out.last_mut(), op)
                    && *first + *count == *next
                    && *clip == *next_clip
                {
                    *count += *n;
                } else {
                    out.push(op.clone());
                }
            }
            out
        };
        assert_eq!(coalesce(&with_splits.ops), coalesce(&plain.ops));
        assert_eq!(splits.len(), 2);
        let first = splits[0].expect("first panel is ordered");
        let second = splits[1].expect("second panel is ordered");
        assert!(first < second);
        for split in [first, second] {
            assert!(matches!(
                with_splits.ops.get(split),
                Some(Op::Stencil { clip: Some(_), .. })
            ));
            assert!(matches!(
                with_splits.ops.get(split - 1),
                Some(Op::Text { .. })
            ));
        }
        // A later grid makes a safe insertion impossible; do not claim a boundary there.
        g.panels[0].items.push(grid);
        let (_, unordered) = build_for_host(&g, &State::default(), 1.0);
        assert_eq!(unordered[0], None);
        assert!(unordered[1].is_some());
    }

    #[test]
    fn analytic_circles_are_default_and_reduce_only_eligible_marks() {
        let mut g = chart(
            json!({"type":"scatter", "title":"T", "encoding":{"x":"x","y":"y"}}),
            &[json!({"x":1,"y":2})],
        );
        g.figure_items.clear();
        let point = Item::Point {
            center: [100.25, 80.5],
            radius: 4.0,
            shape: Shape::Circle,
            fill: Some(Fill::solid(Rgba([0.3, 0.4, 0.5, 0.6]))),
            stroke: None,
            series: Some(1),
            obs: None,
            role: Role::Mark,
        };
        g.panels[0].items = vec![point.clone()];
        g.entrance = None;
        for scale in [1.0, 2.0] {
            let default = build(&g, &State::emphasis(Some(0)), scale);
            let off = build_with_analytic_circles(&g, &State::emphasis(Some(0)), scale, false);
            assert_eq!(off.vertices.len() / FLOATS, 90);
            let on = build_with_analytic_circles(&g, &State::emphasis(Some(0)), scale, true);
            assert_eq!(default, on);
            assert_eq!(on.vertices.len() / FLOATS, 6);
            assert_eq!(on.texts, off.texts);
            for v in on.vertices.as_chunks::<FLOATS>().0 {
                assert_eq!(&v[6..], &[100.25, 80.5, ANALYTIC_CIRCLE, 4.0]);
                assert!(
                    (v[5] - 0.12).abs() < 1e-6,
                    "retain straight alpha and dimming"
                );
                assert!((f64::from(v[0]) - 100.25).abs() <= 4.0 + 0.5 / scale);
                assert!((f64::from(v[1]) - 80.5).abs() <= 4.0 + 0.5 / scale);
            }
            let clip_of = |m: &Mesh| match m.ops[0] {
                Op::Tris { clip, .. } => clip,
                _ => panic!("point solid run"),
            };
            assert_eq!(clip_of(&on), clip_of(&off));
        }
        for entrance in [
            richview_chart::geometry::Entrance::Sweep,
            richview_chart::geometry::Entrance::Settle,
            richview_chart::geometry::Entrance::Fan,
        ] {
            g.entrance = Some(entrance);
            let off = build_with_analytic_circles(&g, &State::default(), 1.0, false);
            let on = build_with_analytic_circles(&g, &State::default(), 1.0, true);
            assert_eq!(on.vertices[8] - ANALYTIC_CIRCLE, off.vertices[8]);
        }
        g.entrance = None;
        for variant in 0..5 {
            let mut fallback = point.clone();
            if let Item::Point {
                radius,
                shape,
                stroke,
                fill,
                ..
            } = &mut fallback
            {
                match variant {
                    0 => *radius = 0.5,
                    1 => *shape = Shape::Square,
                    2 => *stroke = Some(Stroke::solid(Rgba([0.0, 0.0, 0.0, 1.0]), 1.5)),
                    3 => *fill = None,
                    _ => fill.as_mut().unwrap().hatch = Some(Hatch::Diagonal),
                }
            }
            g.panels[0].items = vec![fallback];
            assert_eq!(
                build(&g, &State::default(), 2.0),
                build_with_analytic_circles(&g, &State::default(), 2.0, false),
                "fallback {variant}"
            );
        }
    }

    #[test]
    fn round_joins_keep_every_original_vertex_bit_at_all_tessellation_counts() {
        // The former per-join calculation is the reference. In particular, the final
        // sine(TAU) sample is not replaced by zero or by the first sample.
        for scale in [0.5, 1.0, 1.5, 2.0, 3.0] {
            for segments in 8..=48 {
                let radius = ((segments as f64 + 0.25) / 4.0).powi(2) / (scale * 2.0);
                let steps = ((radius * scale * 2.0).sqrt() * 4.0).clamp(8.0, 48.0) as usize;
                assert_eq!(steps, segments);
                for centre in [[0.0, 0.0], [1e-14, -1e-14], [120.25, -80.33], [1e7, -1e7]] {
                    let paint = Paint {
                        color: [0.3, 0.4, 0.5, 0.6],
                        anchor: [-5.0, 9.0],
                        motion: Motion::GrowX,
                        delay: 1.3,
                    };
                    let clip = Some(Rect::new(-1.0, -2.0, 50.0, 70.0));
                    let mut actual = Builder {
                        mesh: Mesh::default(),
                        scale,
                        open: None,
                        analytic_circles: false,
                    };
                    actual.tris(clip);
                    actual.disc(centre, radius, &paint);
                    actual.close();
                    let mut expected = Builder {
                        mesh: Mesh::default(),
                        scale,
                        open: None,
                        analytic_circles: false,
                    };
                    expected.tris(clip);
                    for i in 0..steps {
                        let (a0, a1) = (
                            i as f64 / steps as f64 * TAU,
                            (i + 1) as f64 / steps as f64 * TAU,
                        );
                        expected.tri(
                            centre,
                            [centre[0] + radius * a0.cos(), centre[1] + radius * a0.sin()],
                            [centre[0] + radius * a1.cos(), centre[1] + radius * a1.sin()],
                            &paint,
                        );
                    }
                    expected.close();
                    assert_eq!(
                        actual
                            .mesh
                            .vertices
                            .iter()
                            .map(|v| v.to_bits())
                            .collect::<Vec<_>>(),
                        expected
                            .mesh
                            .vertices
                            .iter()
                            .map(|v| v.to_bits())
                            .collect::<Vec<_>>(),
                        "scale={scale}, segments={segments}, centre={centre:?}"
                    );
                    assert_eq!(actual.mesh.ops, expected.mesh.ops);
                    assert_eq!(actual.mesh.texts, expected.mesh.texts);
                }
            }
        }
    }

    #[test]
    fn analytic_round_discs_preserve_other_motion_anchors_and_fan_reference() {
        for motion in [
            Motion::Still,
            Motion::Sweep,
            Motion::Settle,
            Motion::Fan,
            Motion::GrowX,
            Motion::GrowY,
            Motion::Late,
            Motion::Scale,
            Motion::Pop,
            Motion::Wash,
        ] {
            let paint = Paint {
                color: [0.3, 0.4, 0.5, 0.6],
                anchor: [-7.0, 3.0],
                motion,
                delay: 32.0,
            };
            let disc = |enabled| {
                let mut b = Builder {
                    mesh: Mesh::default(),
                    scale: 2.0,
                    open: None,
                    analytic_circles: enabled,
                };
                b.tris(Some(Rect::new(0.0, 0.0, 90.0, 80.0)));
                b.disc([12.25, 33.5], 1.25, &paint);
                b.close();
                b.mesh
            };
            let off = disc(false);
            let on = disc(true);
            if matches!(motion, Motion::Still | Motion::Sweep | Motion::Settle) {
                assert_eq!(on.vertices.len() / FLOATS, 6);
                for v in on.vertices.as_chunks::<FLOATS>().0 {
                    assert_eq!(&v[2..6], &paint.color);
                    assert_eq!(&v[6..8], &[12.25, 33.5]);
                    assert_eq!(v[8], ANALYTIC_CIRCLE + motion as u8 as f32);
                    assert_eq!(v[9], 1.25);
                }
            } else {
                assert_eq!(on, off, "retain anchor/delay for {motion:?}");
            }
        }
    }

    #[test]
    fn dashes_split_a_line_by_the_pattern() {
        let pieces = dash(&[[0.0, 0.0], [20.0, 0.0]], &[6.0, 3.0]);
        assert_eq!(pieces.len(), 3);
        assert_eq!(pieces[0], vec![[0.0, 0.0], [6.0, 0.0]]);
        assert_eq!(pieces[1][0], [9.0, 0.0]);
        assert_eq!(pieces[2].last(), Some(&[20.0, 0.0]));
    }

    #[test]
    fn rounded_corners_only_where_asked() {
        let ring = rounded_ring(Rect::new(0.0, 0.0, 10.0, 20.0), [3.0, 3.0, 0.0, 0.0], 0.25);
        assert!(
            ring.contains(&[10.0, 20.0]) && ring.contains(&[0.0, 20.0]),
            "square bottom corners"
        );
        assert!(!ring.contains(&[0.0, 0.0]), "rounded top-left");
        assert!(
            ring.iter().all(|p| p[0] >= -1e-9
                && p[0] <= 10.0 + 1e-9
                && p[1] >= -1e-9
                && p[1] <= 20.0 + 1e-9)
        );
    }

    #[test]
    fn every_item_draws_in_paint_order() {
        let g = chart(
            json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}}),
            &[json!({"c": "A", "v": 3}), json!({"c": "B", "v": 5})],
        );
        let mesh = build(&g, &State::default(), 2.0);
        assert_eq!(mesh.vertices.len() % FLOATS, 0);
        let texts = g
            .panels
            .iter()
            .flat_map(|p| &p.items)
            .chain(&g.figure_items)
            .filter(|i| matches!(i, Item::Text(_)))
            .count();
        assert_eq!(mesh.texts.len(), texts);
        let total = (mesh.vertices.len() / FLOATS) as u32;
        for op in &mesh.ops {
            match op {
                Op::Tris { first, count, .. } => assert!(first + count <= total),
                Op::Stencil { fan, cover, .. } => {
                    assert!(fan.0 + fan.1 <= total && cover.0 + cover.1 <= total)
                }
                Op::Text { index, .. } => assert!((*index as usize) < mesh.texts.len()),
            }
        }
    }

    #[test]
    fn bars_start_evenly_across_the_style_spread_under_the_cap() {
        let rows: Vec<_> = (0..12)
            .map(|i| json!({"c": format!("c{i}"), "v": i + 1}))
            .collect();
        let spec = json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}});
        let delays = |g: &Geometry| -> Vec<f32> {
            let mut out: Vec<f32> = build(g, &State::default(), 1.0)
                .vertices
                .chunks(FLOATS)
                .filter(|v| v[8] == Motion::GrowY as u8 as f32)
                .map(|v| v[9])
                .collect();
            out.sort_by(f32::total_cmp);
            out.dedup();
            out
        };
        let mut g = chart(spec, &rows);
        g.timing.stagger_ms = 20.0;
        let at_20: Vec<f32> = (0..12).map(|i| i as f32 * 20.0).collect();
        assert_eq!(delays(&g), at_20, "under the cap the style's step stands");
        g.timing.stagger_ms = 40.0;
        let at_40 = delays(&g);
        assert_eq!(at_40.len(), 12);
        assert_eq!(at_40.last(), Some(&320.0), "the whole stagger stays capped");
        assert!(
            at_40
                .windows(2)
                .all(|w| (w[1] - w[0] - 320.0 / 11.0).abs() < 1e-3),
            "past the cap every bar still starts after the one before: {at_40:?}"
        );
        g.timing.stagger_ms = 0.0;
        assert_eq!(delays(&g), [0.0], "crisp marks start together");
    }

    #[test]
    fn tiles_grow_from_their_own_centre_in_a_wave_and_points_pop() {
        let rows: Vec<_> = (0..4)
            .flat_map(|c| {
                (0..3).map(
                    move |r| json!({"c": format!("c{c}"), "r": format!("r{r}"), "v": c * 3 + r}),
                )
            })
            .collect();
        let g = chart(
            json!({"type": "heatmap", "title": "T", "encoding": {"x": "c", "y": "r", "color": "v"}}),
            &rows,
        );
        let plot = g.panels[0].plot;
        let tiles: Vec<[f32; 4]> = build(&g, &State::default(), 1.0)
            .vertices
            .chunks(FLOATS)
            .filter(|v| v[8] == Motion::Scale as u8 as f32)
            .map(|v| [v[0], v[1], v[6], v[7]])
            .collect();
        assert!(tiles.len() >= 12 * 6, "every cell grows: {}", tiles.len());
        for [x, y, ax, ay] in &tiles {
            assert!((x - ax).abs() < plot.w as f32 / 4.0 && (y - ay).abs() < plot.h as f32 / 3.0);
        }
        let delays: Vec<f32> = build(&g, &State::default(), 1.0)
            .vertices
            .chunks(FLOATS)
            .filter(|v| v[8] == Motion::Scale as u8 as f32)
            .map(|v| v[9])
            .collect();
        let (first, last) = delays
            .iter()
            .fold((f32::MAX, 0.0f32), |(a, b), d| (a.min(*d), b.max(*d)));
        assert!(
            first < last && last <= g.timing.spread(12) as f32,
            "a wave within the spread"
        );
        let points = chart(
            json!({"type": "scatter", "title": "T", "encoding": {"x": "c", "y": "v"}}),
            &[json!({"c": 1, "v": 2}), json!({"c": 2, "v": 3})],
        );
        assert!(
            build(&points, &State::default(), 1.0)
                .vertices
                .chunks(FLOATS)
                .any(|v| v[8] == Motion::Pop as u8 as f32
                    || v[8] == ANALYTIC_CIRCLE + Motion::Pop as u8 as f32),
            "points grow in from half their radius"
        );
    }

    #[test]
    fn hollow_points_fade_in_place() {
        let rows: Vec<_> = ["A", "B", "C"]
            .iter()
            .enumerate()
            .map(|(i, c)| json!({"c": c, "before": 50 + i * 5, "after": 70 + i * 4}))
            .collect();
        let g = chart(
            json!({"type": "dumbbell", "title": "T", "encoding": {"x": "c", "lower": "before", "upper": "after"}}),
            &rows,
        );
        let points: Vec<&Item> = g.panels[0]
            .items
            .iter()
            .filter(|i| matches!(i, Item::Point { .. }) && i.role() == Role::Mark)
            .collect();
        assert!(
            points.iter().any(|p| !pops(&g, p)) && points.iter().any(|p| pops(&g, p)),
            "the dumbbell draws hollow and filled points"
        );
        let baseline = baseline_of(&g.panels[0].items);
        let tiles = tiles(&g, &g.panels[0].items, Some(g.panels[0].plot));
        for (item, tile) in g.panels[0].items.iter().zip(&tiles) {
            let (motion, ..) = motion_of(&g, item, baseline, *tile, 0, points.len());
            match item {
                Item::Point { .. } if item.role() == Role::Mark && !pops(&g, item) => {
                    assert_ne!(motion, Motion::Pop, "a hollow point never grows")
                }
                _ => {}
            }
        }
    }

    #[test]
    fn a_treemap_group_grows_with_the_tiles_inside_it() {
        let rows = [
            json!({"part": "Engineering"}),
            json!({"part": "Platform", "group": "Engineering", "budget": 420}),
            json!({"part": "Product", "group": "Engineering", "budget": 310}),
            json!({"part": "Sales"}),
            json!({"part": "EMEA", "group": "Sales", "budget": 240}),
            json!({"part": "Americas", "group": "Sales", "budget": 260}),
            json!({"part": "People", "budget": 90}),
        ];
        let g = chart(
            json!({"type": "treemap", "title": "T", "encoding": {"label": "part", "parent": "group", "y": "budget"}}),
            &rows,
        );
        let mesh = build(&g, &State::default(), 1.0);
        let mut starts: Vec<([i32; 2], i32)> = mesh
            .vertices
            .chunks(FLOATS)
            .filter(|v| v[8] == Motion::Scale as u8 as f32)
            .map(|v| {
                (
                    [v[6].round() as i32, v[7].round() as i32],
                    v[9].round() as i32,
                )
            })
            .collect();
        starts.sort();
        starts.dedup();
        assert_eq!(
            starts.len(),
            3,
            "Engineering, Sales and People each grow from one centre at one time: {starts:?}"
        );
    }

    #[test]
    fn a_funnel_widens_from_its_axis_and_a_box_waits_only_for_the_box_before_it() {
        let rows: Vec<_> = [("Visited", 1200), ("Signed up", 400), ("Paid", 90)]
            .iter()
            .map(|(s, v)| json!({"stage": s, "n": v}))
            .collect();
        let g = chart(
            json!({"type": "funnel", "title": "T", "encoding": {"label": "stage", "y": "n"}}),
            &rows,
        );
        let mesh = build(&g, &State::default(), 1.0);
        let moving: Vec<&[f32]> = mesh
            .vertices
            .chunks(FLOATS)
            .filter(|v| v[8] != Motion::Still as u8 as f32 && v[8] != Motion::Late as u8 as f32)
            .collect();
        assert!(!moving.is_empty());
        assert!(
            moving.iter().all(|v| v[8] == Motion::GrowX as u8 as f32),
            "every stage and joint widens from the axis"
        );
        assert_eq!(
            moving.iter().map(|v| v[9]).fold(f32::MAX, f32::min),
            0.0,
            "the first starts at once"
        );
        let rows: Vec<_> = (0..20)
            .map(|i| json!({"g": if i % 2 == 0 { "a" } else { "b" }, "v": 100 + i * 7}))
            .collect();
        let boxes = chart(
            json!({"type": "boxplot", "title": "T", "encoding": {"x": "g", "y": "v"}}),
            &rows,
        );
        let last = build(&boxes, &State::default(), 1.0)
            .vertices
            .chunks(FLOATS)
            .filter(|v| v[8] == Motion::Scale as u8 as f32)
            .map(|v| v[9])
            .fold(0.0f32, f32::max);
        let count = stagger_count(&boxes);
        assert!(
            count <= 4,
            "two boxes are a short wave, not the whole cap: {count}"
        );
        assert!(
            last <= boxes.timing.spread(count) as f32 + 1e-3,
            "two boxes: {last} ms"
        );
    }

    #[test]
    fn bars_grow_from_the_zero_line_and_emphasis_dims_other_series() {
        let rows: Vec<_> = ["A", "B"]
            .iter()
            .flat_map(|c| ["x", "y"].map(|s| json!({"c": c, "v": 2, "s": s})))
            .collect();
        let g = chart(
            json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v", "color": "s"}}),
            &rows,
        );
        let mesh = build(&g, &State::emphasis(Some(0)), 1.0);
        let grow = mesh
            .vertices
            .chunks(FLOATS)
            .filter(|v| v[8] == Motion::GrowY as u8 as f32)
            .count();
        assert!(grow >= 24, "four bars of two triangles each grow: {grow}");
        let alphas: Vec<f32> = mesh
            .vertices
            .chunks(FLOATS)
            .filter(|v| v[8] == Motion::GrowY as u8 as f32)
            .map(|v| v[5])
            .collect();
        assert!(
            alphas.iter().any(|a| (*a - 0.2).abs() < 1e-6)
                && alphas.iter().any(|a| (*a - 1.0).abs() < 1e-6)
        );
    }

    #[test]
    fn an_emphasised_bar3d_row_is_painted_again_over_the_rows_that_step_back() {
        let input: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../richview-chart/tests/fixtures/types/bar3d.json"
        )))
        .unwrap();
        let mut g = chart(input["spec"].clone(), input["rows"].as_array().unwrap());
        // The panel's items end the mesh, so the second painting is its tail.
        g.figure_items.clear();
        let row = g.panels[0].observations[0].series.expect("a bar has a row");
        let state = |dim: f32| State {
            emphasis: Some(Focus::SeriesOf {
                series: row,
                panel: 0,
                observation: 0,
            }),
            dim,
            floor: DIM_FLOOR,
            node_floor: DIM_FLOOR,
            arrive: 1.0,
            leaving: None,
        };
        let rest = build(&g, &State::default(), 1.0).vertices;
        // The row's faces and hairlines alone, in the model's order.
        let mut only = g.clone();
        only.panels[0]
            .items
            .retain(|item| item.series() == Some(row));
        let kept = build(&only, &State::default(), 1.0).vertices;
        assert!(!kept.is_empty() && kept.len() < rest.len());
        // Before the others step back nothing is added: the drawing is the model's.
        assert_eq!(build(&g, &state(0.0), 1.0).vertices.len(), rest.len());
        for dim in [0.5_f32, 1.0] {
            let mesh = build(&g, &state(dim), 1.0).vertices;
            assert_eq!(mesh.len(), rest.len() + kept.len(), "dim {dim}");
            // The copy is the row itself, as opaque as the others have stepped back.
            for (over, own) in mesh[rest.len()..].chunks(FLOATS).zip(kept.chunks(FLOATS)) {
                for k in 0..FLOATS {
                    let expected = if k == 5 { own[k] * dim } else { own[k] };
                    assert!(
                        (over[k] - expected).abs() < 1e-6,
                        "dim {dim}, float {k}: {} for {expected}",
                        over[k]
                    );
                }
            }
        }
        // Other chart types keep one painting: their marks do not hide each other.
        let rows: Vec<_> = ["A", "B"]
            .iter()
            .flat_map(|c| ["x", "y"].map(|s| json!({"c": c, "v": 2, "s": s})))
            .collect();
        let bar = chart(
            json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v", "color": "s"}}),
            &rows,
        );
        assert_eq!(
            build(&bar, &State::emphasis(Some(0)), 1.0).vertices.len(),
            build(&bar, &State::default(), 1.0).vertices.len()
        );
    }

    #[test]
    fn a_flow_keeps_the_hovered_ribbon_or_node_and_what_it_connects() {
        let rows = [
            json!({"from": "A", "to": "C", "v": 3}),
            json!({"from": "B", "to": "C", "v": 2}),
            json!({"from": "C", "to": "D", "v": 4}),
            json!({"from": "B", "to": "E", "v": 1}),
        ];
        let g = chart(
            json!({"type": "sankey", "title": "T", "encoding": {"source": "from", "target": "to", "y": "v"}}),
            &rows,
        );
        let panel = &g.panels[0];
        let named = |name: &str| {
            panel
                .observations
                .iter()
                .position(|o| o.readout.heading == name)
                .unwrap_or_else(|| panic!("no observation {name}"))
        };
        let names = |keep: Vec<usize>| {
            let mut names: Vec<&str> = keep
                .iter()
                .map(|k| panel.observations[*k].readout.heading.as_str())
                .collect();
            names.sort_unstable();
            names
        };
        // A ribbon keeps itself and its two nodes.
        assert_eq!(names(connected(panel, named("A → C"))), ["A", "A → C", "C"]);
        // A node keeps its ribbons and the nodes they reach, and nothing further on.
        assert_eq!(
            names(connected(panel, named("B"))),
            ["B", "B → C", "B → E", "C", "E"]
        );
        assert_eq!(
            names(connected(panel, named("C"))),
            ["A", "A → C", "B", "B → C", "C", "C → D", "D"]
        );
    }

    #[test]
    fn a_stream_layer_is_found_under_the_pointer() {
        let rows: Vec<serde_json::Value> = (0..6)
            .flat_map(|t| ["a", "b", "c"].map(|s| json!({"t": t, "s": s, "v": 2 + t % 3})))
            .collect();
        let g = chart(
            json!({"type": "streamgraph", "title": "T", "encoding": {"x": "t", "y": "v", "color": "s"}}),
            &rows,
        );
        let panel = &g.panels[0];
        // Each observation's dots sit at the middle of their layers.
        let o = &panel.observations[2];
        assert!(!o.dots.is_empty(), "layers carry dots at their middles");
        for dot in &o.dots {
            assert_eq!(layer_at(panel, dot.at), Some(dot.series));
        }
        assert_eq!(
            layer_at(panel, [panel.plot.x + 1.0, panel.plot.y - 20.0]),
            None
        );
    }

    /// The alpha of each wedge of a two-slice pie under `state`, in slice order.
    fn wedge_alphas(state: &State) -> Vec<f32> {
        let g = chart(
            json!({"type": "pie", "title": "T", "encoding": {"x": "c", "y": "v"}}),
            &[json!({"c": "A", "v": 1}), json!({"c": "B", "v": 3})],
        );
        g.panels[0]
            .items
            .iter()
            .filter(|i| {
                matches!(
                    i,
                    Item::Area {
                        role: Role::Mark,
                        obs: Some(_),
                        ..
                    }
                )
            })
            .map(|i| state.dim_of(0, i))
            .map(|dim| 1.0 - dim)
            .collect()
    }

    #[test]
    fn a_hovered_mark_keeps_its_strength_and_the_other_marks_step_back() {
        let marks = |keep: Vec<usize>| Focus::Marks { panel: 0, keep };
        let settled = State {
            emphasis: Some(marks(vec![1])),
            dim: 1.0,
            floor: DIM_FLOOR,
            node_floor: DIM_FLOOR,
            arrive: 1.0,
            leaving: None,
        };
        let alphas = wedge_alphas(&settled);
        assert!(
            (alphas[0] - 0.2).abs() < 1e-6 && alphas[1] == 1.0,
            "{alphas:?}"
        );
        // Another panel's marks and marks that own no observation never dim.
        let elsewhere = State {
            emphasis: Some(Focus::Marks {
                panel: 1,
                keep: vec![1],
            }),
            ..settled.clone()
        };
        assert_eq!(wedge_alphas(&elsewhere), [1.0, 1.0]);
        // Moving from slice 0 to slice 1: the one left still holds half its strength and the
        // one reached has come a quarter of the way back.
        let moving = State {
            arrive: 0.25,
            leaving: Some((marks(vec![0]), 0.5)),
            ..settled.clone()
        };
        let alphas = wedge_alphas(&moving);
        assert!(
            (alphas[0] - 0.6).abs() < 1e-6 && (alphas[1] - 0.4).abs() < 1e-6,
            "{alphas:?}"
        );
        // A flow keeps a tenth, and a release halfway restores half of what was taken.
        let flow = State {
            floor: FLOW_DIM_FLOOR,
            dim: 0.5,
            ..settled
        };
        assert!((wedge_alphas(&flow)[0] - 0.55).abs() < 1e-6);
        // A hovered observation's own mark of another series stays with it.
        let point = |obs| Item::Point {
            center: [0.0; 2],
            radius: 3.0,
            shape: Shape::Circle,
            fill: Some(Fill::solid(Rgba([0.0, 0.0, 0.0, 1.0]))),
            stroke: None,
            role: Role::Mark,
            series: Some(1),
            obs: Some(obs),
        };
        let hovered = State {
            emphasis: Some(Focus::SeriesOf {
                series: 0,
                panel: 0,
                observation: 2,
            }),
            dim: 1.0,
            floor: DIM_FLOOR,
            node_floor: DIM_FLOOR,
            arrive: 1.0,
            leaving: None,
        };
        assert_eq!(hovered.dim_of(0, &point(2)), 0.0);
        assert!((hovered.dim_of(0, &point(3)) - 0.8).abs() < 1e-6);
        assert!((hovered.dim_of(1, &point(2)) - 0.8).abs() < 1e-6);
        // A flow's unconnected node keeps a fifth while its ribbons keep a tenth.
        let node = Item::Rect {
            rect: Rect::new(0.0, 0.0, 10.0, 40.0),
            fill: Fill::solid(Rgba([0.0, 0.0, 0.0, 1.0])),
            stroke: None,
            radius: [0.0; 4],
            role: Role::Mark,
            series: None,
            obs: Some(0),
        };
        let settled_flow = State { dim: 1.0, ..flow };
        assert!((settled_flow.dim_of(0, &node) - 0.8).abs() < 1e-6);
        assert!((wedge_alphas(&settled_flow)[0] - 0.1).abs() < 1e-6);
        let dark_flow = State {
            floor: page_floor(FLOW_DIM_FLOOR, "oklch(0.2 0 0)"),
            node_floor: page_floor(DIM_FLOOR, "oklch(0.2 0 0)"),
            ..settled_flow
        };
        assert!((1.0 - dark_flow.dim_of(0, &node) - 0.32).abs() < 1e-6);
        assert!((wedge_alphas(&dark_flow)[0] - 0.16).abs() < 1e-6);
    }

    #[test]
    fn pie_slices_fill_by_stencil_and_fan_from_the_centre() {
        let g = chart(
            json!({"type": "pie", "title": "T", "encoding": {"x": "c", "y": "v"}}),
            &[json!({"c": "A", "v": 1}), json!({"c": "B", "v": 3})],
        );
        let mesh = build(&g, &State::default(), 1.0);
        assert_eq!(
            mesh.ops
                .iter()
                .filter(|o| matches!(o, Op::Stencil { .. }))
                .count(),
            2
        );
        let center = g.panels[0].observations[0].wedge.unwrap().center;
        assert!(mesh.vertices.chunks(FLOATS).any(|v| v[8] == Motion::Fan as u8 as f32 && (v[6] - center[0] as f32).abs() < 1e-3));
    }

    #[test]
    fn a_style_without_motion_keeps_every_vertex_still() {
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v"}}),
        )
        .unwrap();
        let rows = vec![
            json!({"m": "2024-01", "v": 1}),
            json!({"m": "2024-02", "v": 2}),
        ];
        let g = layout(
            &block,
            &rows,
            &ChartTheme {
                motion: false,
                ..ChartTheme::default()
            },
            &Options {
                width: 640.0,
                ..Options::default()
            },
        );
        let mesh = build(&g, &State::default(), 1.0);
        assert!(
            mesh.vertices
                .chunks(FLOATS)
                .all(|v| v[8] == 0.0 || v[8] == ANALYTIC_CIRCLE)
        );
    }
}
