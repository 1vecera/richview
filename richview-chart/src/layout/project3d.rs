//! What the three-dimensional types share (`surface`, `scatter3d`, `bar3d`): an orthographic
//! projection of the unit cube onto the page, the box with its walls, grid and tick labels, and the
//! fit of the whole box with its labels into a frame. A painter maps its data into the cube
//! (each coordinate from -0.5 to 0.5), projects it with [`Scene::at`] and draws far marks first
//! (painter's algorithm), so the 3-D chart is plain 2-D items and every renderer and check works
//! on it unchanged (FEA-CH-75). The view comes from `style.azimuth` and `style.elevation`, or from
//! the reader's own angles in [`Options::view`](super::Options::view); a host turns the chart by
//! laying it out again with new angles from [`turn_by_pixels`] and [`turn_by_degrees`], so every
//! angle goes through this one projection.

use super::{Ctx, GRID_WIDTH, Style};
use crate::axis::{self, ValueSpec, Zero};
use crate::block::ChartBlock;
use crate::color::{self, Oklch};
use crate::det::{hypot, sin_cos};
use crate::geometry::{Anchor, DropReason, Face, Fill, Item, Path, Rect, Role, Stroke, Text};
use crate::numbers::Format;
use std::cell::RefCell;

/// Degrees the view turns round the vertical axis when the author gives none.
pub(super) const DEFAULT_AZIMUTH: f64 = 35.0;
/// Degrees the view looks down when the author gives none.
pub(super) const DEFAULT_ELEVATION: f64 = 28.0;
/// The lowest view, in degrees: lower and the floor would flatten to a line.
pub const MIN_ELEVATION: f64 = 5.0;
/// The highest view, in degrees: higher and the box would look straight down.
pub const MAX_ELEVATION: f64 = 85.0;
/// Degrees the view turns for each pixel a pointer is dragged: a drag across 720 px is one full
/// turn round the vertical axis, and 160 px take the view from its lowest to its highest.
const DEGREES_PER_PIXEL: f64 = 0.5;
/// Degrees one key press turns the view.
pub const KEY_TURN: f64 = 5.0;
/// Gap between a box edge and its tick labels.
const TICK_GAP: f64 = 7.0;
/// Length of a tick mark.
const TICK_LEN: f64 = 4.0;
/// How much further from the vertical axis its tick labels stand when the view looks straight
/// down: there the axis is short and the surface's corner meets it, so the labels need the room.
const HIGH_VIEW_GAP: f64 = 6.0;
/// The tick mark stops this far short of its label.
const TICK_TO_LABEL: f64 = 3.0;
/// The elevations the fit tries, in degrees; see [`fit_views`] for the azimuths.
const FIT_ELEVATIONS: [f64; 5] = [MIN_ELEVATION, 20.0, 40.0, 62.0, MAX_ELEVATION];
/// How far the unit cube's silhouette reaches from its centre at any angle, in cube edges: across
/// (a corner turned square to the viewer, half the floor's diagonal) and up or down (the corner
/// reached at an elevation of about 55 degrees, half the cube's diagonal).
const HALF_ACROSS: f64 = std::f64::consts::FRAC_1_SQRT_2;
const HALF_HIGH: f64 = 0.866_025_403_784_438_6;
/// How much of its tight size the fit gives the box: the angles it tries are a sample, and a view
/// between them may need a little more room.
const FIT_MARGIN: f64 = 0.98;
/// The smallest cube edge, in pixels, the fit gives a box.
const MIN_SCALE: f64 = 12.0;

/// The direction the chart is seen from.
#[derive(Debug, Clone, Copy)]
pub(super) struct View {
    sin_a: f64,
    cos_a: f64,
    sin_e: f64,
    cos_e: f64,
    elevation: f64,
}

/// The view a block asks for as `[azimuth, elevation]` in degrees, the azimuth within 0 and 360 and
/// the elevation within [`MIN_ELEVATION`] and [`MAX_ELEVATION`]: where a reader's turning starts,
/// and where the reset returns to.
pub fn authored_view(block: &ChartBlock) -> [f64; 2] {
    let finite = |v: Option<f64>, default: f64| v.filter(|x| x.is_finite()).unwrap_or(default);
    clamp_view([
        finite(block.style_number("azimuth"), DEFAULT_AZIMUTH),
        finite(block.style_number("elevation"), DEFAULT_ELEVATION),
    ])
}

/// `view` as the projection takes it: the azimuth wrapped into 0 to 360, the elevation held
/// between [`MIN_ELEVATION`] and [`MAX_ELEVATION`], a number that is not finite as the default.
fn clamp_view(view: [f64; 2]) -> [f64; 2] {
    let [azimuth, elevation] = view;
    [
        if azimuth.is_finite() {
            azimuth.rem_euclid(360.0)
        } else {
            DEFAULT_AZIMUTH
        },
        if elevation.is_finite() {
            elevation.clamp(MIN_ELEVATION, MAX_ELEVATION)
        } else {
            DEFAULT_ELEVATION
        },
    ]
}

/// `view` turned by `degrees` as `[azimuth, elevation]`: a positive azimuth brings the near side
/// of the box to the right, a positive elevation looks down from higher. The elevation stops at
/// its limits; the azimuth goes round.
pub fn turn_by_degrees(view: [f64; 2], degrees: [f64; 2]) -> [f64; 2] {
    clamp_view([view[0] + degrees[0], view[1] + degrees[1]])
}

/// `view` turned by a pointer dragged `pixels` as `[right, down]`: the box follows the pointer, as
/// if its near face and its top were held and pulled along.
pub fn turn_by_pixels(view: [f64; 2], pixels: [f64; 2]) -> [f64; 2] {
    turn_by_degrees(view, [pixels[0] * DEGREES_PER_PIXEL, pixels[1] * DEGREES_PER_PIXEL])
}

/// The view `t` of the way (0 to 1) from `from` to `to`: the azimuth the shorter way round, the
/// elevation straight. A host glides a reset back to the author's view by it.
pub fn between_views(from: [f64; 2], to: [f64; 2], t: f64) -> [f64; 2] {
    let (from, to) = (clamp_view(from), clamp_view(to));
    let mut round = (to[0] - from[0]).rem_euclid(360.0);
    if round > 180.0 {
        round -= 360.0;
    }
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 1.0 };
    clamp_view([from[0] + round * t, from[1] + (to[1] - from[1]) * t])
}

/// A projected point: page offsets (y down) and how near the viewer it is (larger is nearer).
#[derive(Debug, Clone, Copy)]
pub(super) struct Pt {
    pub x: f64,
    pub y: f64,
    pub depth: f64,
}

impl View {
    /// The view from `azimuth` and `elevation` in degrees; the elevation stays between 5 and 85
    /// so the floor never flattens to a line and the box never looks straight down.
    pub fn new(azimuth: f64, elevation: f64) -> View {
        let elevation = elevation.clamp(MIN_ELEVATION, MAX_ELEVATION);
        // The same projection lays out the chart and its diagnostic regions on native and Wasm.
        let (sin_a, cos_a) = sin_cos(azimuth.rem_euclid(360.0).to_radians());
        let (sin_e, cos_e) = sin_cos(elevation.to_radians());
        View {
            sin_a,
            cos_a,
            sin_e,
            cos_e,
            elevation,
        }
    }

    /// The view the reader has turned the chart to, or else the one the block asks for.
    pub fn of(ctx: &Ctx) -> View {
        let [azimuth, elevation] = ctx
            .opts
            .view
            .filter(|v| v.iter().all(|a| a.is_finite()))
            .unwrap_or_else(|| authored_view(ctx.block));
        View::new(azimuth, elevation)
    }

    /// `p` in the unit cube on the page at unit scale.
    pub fn project(&self, p: [f64; 3]) -> Pt {
        let across = p[0] * self.cos_a - p[1] * self.sin_a;
        let away = p[0] * self.sin_a + p[1] * self.cos_a;
        Pt {
            x: across,
            y: -(p[2] * self.cos_e + away * self.sin_e),
            depth: -away * self.cos_e + p[2] * self.sin_e,
        }
    }

    /// The direction from the cube to the viewer: a face is seen when its outward normal points
    /// along it.
    pub fn toward_viewer(&self) -> [f64; 3] {
        [-self.sin_a * self.cos_e, -self.cos_a * self.cos_e, self.sin_e]
    }
}

/// A view placed on the page: the cube's edge in pixels and where its centre lands.
#[derive(Debug, Clone, Copy)]
pub(super) struct Scene {
    pub view: View,
    pub s: f64,
    pub c: [f64; 2],
}

impl Scene {
    pub fn at(&self, p: [f64; 3]) -> Pt {
        let q = self.view.project(p);
        Pt {
            x: self.c[0] + self.s * q.x,
            y: self.c[1] + self.s * q.y,
            depth: q.depth,
        }
    }
}

/// One axis of the box: labelled ticks at positions in the cube and a title.
#[derive(Debug, Clone)]
pub(super) struct Axis3 {
    pub ticks: Vec<(f64, String)>,
    pub title: String,
}

/// A numeric axis from `lo` to `hi` mapped onto the cube.
#[derive(Debug, Clone, Copy)]
pub(super) struct Lin {
    pub lo: f64,
    pub hi: f64,
}

impl Lin {
    /// Cube coordinate of `v`.
    pub fn t(&self, v: f64) -> f64 {
        let t = crate::scale::position(v, self.lo, self.hi);
        if t.is_finite() { t.clamp(0.0, 1.0) - 0.5 } else { 0.0 }
    }
}

/// About how long a cube edge is on the page in `frame`, which sets how many ticks an axis gets.
pub(super) fn edge_estimate(frame: Rect) -> f64 {
    (frame.w.min(frame.h * 1.5) * 0.6).max(60.0)
}

/// A numeric axis whose limits are round values, with ticks for an edge about `length` px long.
pub(super) fn linear_axis(
    ctx: &Ctx,
    data: [f64; 2],
    zero: Zero,
    length: f64,
    format: &Format,
    title: String,
) -> (Axis3, Lin) {
    let a = axis::value_axis(&ValueSpec {
        data,
        zero,
        min: None,
        max: None,
        length,
        horizontal: false,
        format,
        font: &ctx.style.figures,
        size: ctx.size(),
    });
    let lin = Lin { lo: a.lo, hi: a.hi };
    let ticks = a
        .ticks
        .iter()
        .zip(&a.labels)
        .map(|(v, l)| (lin.t(*v), l.clone()))
        .collect();
    (Axis3 { ticks, title }, lin)
}

/// A category axis: the names at the centres of equal cells.
pub(super) fn category_axis(names: &[String], title: String) -> Axis3 {
    let n = names.len().max(1) as f64;
    Axis3 {
        ticks: names
            .iter()
            .enumerate()
            .map(|(i, name)| ((i as f64 + 0.5) / n - 0.5, name.clone()))
            .collect(),
        title,
    }
}

/// The box as drawn: its items and the labels that did not fit.
pub(super) struct Boxed {
    pub items: Vec<Item>,
    pub dropped: Vec<(String, DropReason, Rect)>,
}

/// Keeps the labels that fit the frame and do not touch one already kept.
struct Placer {
    frame: Option<Rect>,
    kept: Vec<Rect>,
    items: Vec<Item>,
    dropped: Vec<(String, DropReason, Rect)>,
}

impl Placer {
    /// Whether a label at `b` would be kept.
    fn fits(&self, b: &Rect) -> bool {
        self.frame.is_none_or(|f| b.inside(&f, 0.5)) && !self.kept.iter().any(|k| k.overlaps(b, -2.0))
    }

    fn put(&mut self, text: Text) -> bool {
        let b = text.bounds;
        if self.frame.is_some_and(|f| !b.inside(&f, 0.5)) {
            self.dropped.push((text.text.clone(), DropReason::OutOfBounds, b));
            return false;
        }
        if self.kept.iter().any(|k| k.overlaps(&b, -2.0)) {
            self.dropped.push((text.text.clone(), DropReason::Collision, b));
            return false;
        }
        self.kept.push(b);
        self.items.push(Item::Text(text));
        true
    }
}

/// Unit normal of the edge `a`-`b` on the page that points away from the box's centre.
fn outward(scene: &Scene, a: [f64; 3], b: [f64; 3]) -> [f64; 2] {
    let (p, q) = (scene.at(a), scene.at(b));
    let (ex, ey) = (q.x - p.x, q.y - p.y);
    let len = hypot(ex, ey);
    let mid = [(p.x + q.x) / 2.0 - scene.c[0], (p.y + q.y) / 2.0 - scene.c[1]];
    if len < 1e-9 {
        let m = hypot(mid[0], mid[1]);
        return if m < 1e-9 { [0.0, 1.0] } else { [mid[0] / m, mid[1] / m] };
    }
    let mut n = [-ey / len, ex / len];
    if n[0] * mid[0] + n[1] * mid[1] < 0.0 {
        n = [-n[0], -n[1]];
    }
    n
}

/// Page distance between two cube points.
fn span(scene: &Scene, a: [f64; 3], b: [f64; 3]) -> f64 {
    let (p, q) = (scene.at(a), scene.at(b));
    hypot(q.x - p.x, q.y - p.y)
}

/// A label centred beside `at`, pushed out along `n` by `gap` and by half its own extent along `n`.
#[allow(clippy::too_many_arguments)]
fn beside(ctx: &Ctx, text: &str, at: [f64; 2], n: [f64; 2], gap: f64, role: Role, face: Face, color: Oklch) -> Text {
    let probe = ctx.text(text, 0.0, 0.0, Anchor::Middle, face, role, color);
    let off = gap + n[0].abs() * probe.bounds.w / 2.0 + n[1].abs() * probe.bounds.h / 2.0;
    ctx.text_mid(
        text,
        at[0] + n[0] * off,
        at[1] + n[1] * off,
        Anchor::Middle,
        face,
        role,
        color,
    )
}

/// Which tick labels of an axis whose labels are `widest` wide there is room for along an edge of
/// `length` px: every `k`th, starting with the first.
fn thin(count: usize, widest: f64, length: f64) -> usize {
    if count < 2 {
        return 1;
    }
    let pitch = length / count as f64;
    ((widest + 10.0) / pitch.max(1.0)).ceil().max(1.0) as usize
}

fn line(scene: &Scene, a: [f64; 3], b: [f64; 3], stroke: &Stroke, role: Role) -> Item {
    let (p, q) = (scene.at(a), scene.at(b));
    Item::Line {
        path: Path::polyline(&[[p.x, p.y], [q.x, q.y]]),
        stroke: stroke.clone(),
        role,
        series: None,
        obs: None,
    }
}

/// The floor corners that place the labels, as `[x, y]` in the cube: the one nearest the viewer
/// (the walls stand behind it) and the one furthest to the left, where the vertical axis stands.
/// Where two are equally near, the one on the right is the front, so the x and y labels leave the
/// left to the vertical axis; where two are equally far left, the one further away has the axis, so
/// its title stands over the box's back and not over the floor.
fn front_and_left(scene: &Scene) -> ([f64; 2], [f64; 2]) {
    const SAME: f64 = 1e-9;
    let corners = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
    let at = |c: [f64; 2]| scene.view.project([c[0], c[1], -0.5]);
    let mut front = corners[0];
    for c in corners {
        let (p, best) = (at(c), at(front));
        if p.depth > best.depth + SAME || (p.depth > best.depth - SAME && p.x > best.x) {
            front = c;
        }
    }
    let mut left = corners[0];
    for c in corners {
        let (p, best) = (at(c), at(left));
        if p.x < best.x - SAME || (p.x < best.x + SAME && p.depth < best.depth) {
            left = c;
        }
    }
    (front, left)
}

/// The axis titles and tick labels of the box seen as `scene` has it, and the tick marks that go
/// with the labels kept. With a `frame`, labels that would leave it are dropped.
fn labels(
    ctx: &Ctx,
    scene: &Scene,
    axes: &[Axis3; 3],
    frame: Option<Rect>,
    front: [f64; 2],
    left: [f64; 2],
) -> (Vec<Item>, Placer) {
    let edge = Stroke::solid(Style::rgba(ctx.style.ink_faint), 1.0);
    let ink = ctx.style.ink;
    let soft = ctx.style.ink_soft;
    let lh = ctx.line_height();
    let (fx, fy) = (front[0], front[1]);
    let mut placer = Placer {
        frame,
        kept: Vec::new(),
        items: Vec::new(),
        dropped: Vec::new(),
    };
    // Floor edges that carry the x and y labels.
    let x_edge = ([-0.5, fy, -0.5], [0.5, fy, -0.5]);
    let y_edge = ([fx, -0.5, -0.5], [fx, 0.5, -0.5]);
    let widest = |axis: &Axis3| {
        axis.ticks
            .iter()
            .map(|(_, l)| ctx.style.figures.width(l, ctx.size()))
            .fold(0.0, f64::max)
    };
    // Titles first, so a tick label gives way to them and not the reverse.
    let mut tick_ink: Vec<Item> = Vec::new();
    // The vertical axis's title first: it stands over the axis, and the x and y titles give way to it.
    if !axes[2].title.is_empty() {
        let top = scene.at([left[0], left[1], 0.5]);
        let title = ctx.text_mid(
            &axes[2].title,
            top.x,
            top.y - lh * 1.3,
            Anchor::End,
            Face::Data,
            Role::AxisTitle,
            soft,
        );
        placer.put(title);
    }
    for (k, (a, b)) in [x_edge, y_edge].into_iter().enumerate() {
        let axis = &axes[k];
        if axis.title.is_empty() {
            continue;
        }
        let n = outward(scene, a, b);
        let reach = axis
            .ticks
            .iter()
            .map(|(_, l)| {
                let w = ctx.style.figures.width(l, ctx.size());
                n[0].abs() * w + n[1].abs() * lh
            })
            .fold(0.0, f64::max);
        let mid = scene.at([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, -0.5]);
        let gap = TICK_GAP + reach + 4.0;
        let mut title = beside(
            ctx,
            &axis.title,
            [mid.x, mid.y],
            n,
            gap,
            Role::AxisTitle,
            Face::Data,
            soft,
        );
        if !placer.fits(&title.bounds) {
            // Where the edge is seen nearly end-on both titles stand in one place, and at a narrow
            // width a title can reach the frame's side: it slides along its edge, by its own
            // width, to where it fits.
            let along = [n[1], -n[0]];
            let width = title.bounds.w;
            for shift in [0.6, -0.6, 1.2, -1.2] {
                let at = [mid.x + along[0] * shift * width, mid.y + along[1] * shift * width];
                let moved = beside(ctx, &axis.title, at, n, gap, Role::AxisTitle, Face::Data, soft);
                if placer.fits(&moved.bounds) {
                    title = moved;
                    break;
                }
            }
        }
        placer.put(title);
    }
    // Tick labels. Seen from high above, the vertical axis is short and the surface's corner meets
    // its foot, so its labels stand further out the higher the view.
    let z_axis = &axes[2];
    let z_gap = TICK_GAP + HIGH_VIEW_GAP * ((scene.view.elevation - 35.0) / 40.0).clamp(0.0, 1.0);
    for (t, label) in &z_axis.ticks {
        let p = scene.at([left[0], left[1], *t]);
        let text = ctx.text_mid(
            label,
            p.x - z_gap,
            p.y,
            Anchor::End,
            Face::Figures,
            Role::AxisLabel,
            ink,
        );
        if placer.put(text) {
            tick_ink.push(Item::Line {
                path: Path::polyline(&[[p.x - (z_gap - TICK_TO_LABEL).max(TICK_LEN), p.y], [p.x, p.y]]),
                stroke: edge.clone(),
                role: Role::Tick,
                series: None,
                obs: None,
            });
        }
    }
    for (k, (a, b)) in [x_edge, y_edge].into_iter().enumerate() {
        let axis = &axes[k];
        let n = outward(scene, a, b);
        let every = thin(axis.ticks.len(), widest(axis), span(scene, a, b));
        for (i, (t, label)) in axis.ticks.iter().enumerate() {
            if !i.is_multiple_of(every) {
                continue;
            }
            let at = if k == 0 { [*t, a[1], -0.5] } else { [a[0], *t, -0.5] };
            let p = scene.at(at);
            let text = beside(ctx, label, [p.x, p.y], n, TICK_GAP, Role::AxisLabel, Face::Figures, ink);
            if placer.put(text) {
                tick_ink.push(Item::Line {
                    path: Path::polyline(&[[p.x, p.y], [p.x + n[0] * TICK_LEN, p.y + n[1] * TICK_LEN]]),
                    stroke: edge.clone(),
                    role: Role::Tick,
                    series: None,
                    obs: None,
                });
            }
        }
    }
    (tick_ink, placer)
}

/// Draw the box of `axes` (x, y, z): grid on the floor and the two back walls, the edges, tick
/// labels along the two front floor edges and the left vertical edge, and the axis titles. With a
/// `frame`, labels that would leave it are dropped.
pub(super) fn draw_box(ctx: &Ctx, scene: &Scene, axes: &[Axis3; 3], frame: Option<Rect>) -> Boxed {
    let edge = Stroke::solid(Style::rgba(ctx.style.ink_faint), 1.0);
    let grid = Stroke::solid(Style::rgba(ctx.style.rule_soft), GRID_WIDTH);
    let (front, left) = front_and_left(scene);
    // The walls stand behind the front corner.
    let (fx, fy) = (front[0], front[1]);
    let (bx, by) = (-fx, -fy);
    let inner = |t: f64| t.abs() < 0.5 - 1e-9;
    let mut items: Vec<Item> = Vec::new();
    for (t, _) in axes[0].ticks.iter().filter(|(t, _)| inner(*t)) {
        items.push(line(scene, [*t, -0.5, -0.5], [*t, 0.5, -0.5], &grid, Role::Grid));
        items.push(line(scene, [*t, by, -0.5], [*t, by, 0.5], &grid, Role::Grid));
    }
    for (t, _) in axes[1].ticks.iter().filter(|(t, _)| inner(*t)) {
        items.push(line(scene, [-0.5, *t, -0.5], [0.5, *t, -0.5], &grid, Role::Grid));
        items.push(line(scene, [bx, *t, -0.5], [bx, *t, 0.5], &grid, Role::Grid));
    }
    for (t, _) in axes[2].ticks.iter().filter(|(t, _)| inner(*t)) {
        items.push(line(scene, [bx, -0.5, *t], [bx, 0.5, *t], &grid, Role::Grid));
        items.push(line(scene, [-0.5, by, *t], [0.5, by, *t], &grid, Role::Grid));
    }
    let corners = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
    for (i, a) in corners.iter().enumerate() {
        let b = corners[(i + 1) % 4];
        items.push(line(
            scene,
            [a[0], a[1], -0.5],
            [b[0], b[1], -0.5],
            &edge,
            Role::Baseline,
        ));
    }
    for (a, b) in [([bx, by], [fx, by]), ([bx, by], [bx, fy])] {
        items.push(line(
            scene,
            [a[0], a[1], -0.5],
            [a[0], a[1], 0.5],
            &edge,
            Role::Baseline,
        ));
        items.push(line(scene, [a[0], a[1], 0.5], [b[0], b[1], 0.5], &edge, Role::Baseline));
        items.push(line(
            scene,
            [b[0], b[1], -0.5],
            [b[0], b[1], 0.5],
            &edge,
            Role::Baseline,
        ));
    }
    // The vertical axis stands on the floor corner furthest to the left.
    items.push(line(
        scene,
        [left[0], left[1], -0.5],
        [left[0], left[1], 0.5],
        &edge,
        Role::Baseline,
    ));

    let (tick_ink, placer) = labels(ctx, scene, axes, frame, front, left);
    items.extend(tick_ink);
    items.extend(placer.items);
    Boxed {
        items,
        dropped: placer.dropped,
    }
}

/// The views the fit tries, so that one size holds at every angle: round the vertical axis at each
/// of [`FIT_ELEVATIONS`]. The labels jump where another floor corner becomes the front or the left
/// one, every 45 degrees, so each stretch between two jumps is tried just inside both its ends and
/// at two places between.
fn fit_views() -> Vec<View> {
    let mut views = Vec::new();
    for step in 0..8 {
        for inside in [0.25, 15.0, 30.0, 44.75] {
            for elevation in FIT_ELEVATIONS {
                views.push(View::new(f64::from(step) * 45.0 + inside, elevation));
            }
        }
    }
    views
}

/// Where the box seen from `view`, with its tick labels and titles, reaches when its edge is `s`
/// px and its centre is at the origin.
fn extent(ctx: &Ctx, view: View, axes: &[Axis3; 3], s: f64) -> Rect {
    let scene = Scene { view, s, c: [0.0, 0.0] };
    let mut corners = Vec::with_capacity(8);
    for x in [-0.5, 0.5] {
        for y in [-0.5, 0.5] {
            for z in [-0.5, 0.5] {
                corners.push(scene.at([x, y, z]));
            }
        }
    }
    let mut all = bounds_of(&corners);
    let (front, left) = front_and_left(&scene);
    let (ink, placer) = labels(ctx, &scene, axes, None, front, left);
    for b in ink.iter().chain(&placer.items).filter_map(Item::bounds) {
        all = all.union(&b);
    }
    all
}

/// The sizes the last fits settled on, each with what it was fitted to. A chart that turns is laid
/// out again at every frame with the same frame and axes, and its size does not depend on the angle,
/// so only the first layout searches: [`size_for`] returns the same for the same key, and this just
/// keeps its answers.
const FITTED_KEPT: usize = 32;

thread_local! {
    static FITTED: RefCell<Vec<(String, f64)>> = const { RefCell::new(Vec::new()) };
}

/// Everything the size of a box depends on, as text: the frame and padding, the text size, how wide
/// each face sets a sample line, and the axes with their ticks and titles.
fn fit_key(ctx: &Ctx, axes: &[Axis3; 3], frame: Rect, pad: f64) -> String {
    const SAMPLE: &str = "Wi0123456789 (mm/kg/ha)";
    let size = ctx.size();
    format!(
        "{frame:?} {pad:?} {size:?} {:?} {:?} {:?} {axes:?}",
        ctx.line_height(),
        ctx.style.font.width(SAMPLE, size),
        ctx.style.figures.width(SAMPLE, size),
    )
}

/// The largest edge, in pixels, at which every angle the fit tries fits `frame` with its labels and
/// `pad` px round them, with a little to spare for the angles between.
fn size_for(ctx: &Ctx, axes: &[Axis3; 3], frame: Rect, pad: f64) -> f64 {
    let views = fit_views();
    // How much of the frame the worst of the views takes at edge `s`; at most 1 fits.
    let used = |s: f64| {
        views
            .iter()
            .map(|v| {
                let r = extent(ctx, *v, axes, s).inflate(pad);
                (r.w / frame.w.max(1e-9)).max(r.h / frame.h.max(1e-9))
            })
            .fold(0.0, f64::max)
    };
    // The box alone, without its labels, is the ceiling; the labels' room comes off it. The use
    // grows with the size almost in a line, so a few steps between the smallest size and the
    // ceiling find the largest size that fits.
    let ceiling = ((frame.w - 2.0 * pad) / (2.0 * HALF_ACROSS))
        .min((frame.h - 2.0 * pad) / (2.0 * HALF_HIGH))
        .max(MIN_SCALE);
    let top_use = used(ceiling);
    let s = if top_use <= 1.0 {
        ceiling
    } else {
        let low_use = used(MIN_SCALE);
        if low_use > 1.0 {
            // Nothing fits: the labels that leave the frame are dropped.
            MIN_SCALE
        } else {
            let (mut fits, mut over) = ((MIN_SCALE, low_use), (ceiling, top_use));
            for _ in 0..6 {
                let s = fits.0 + (1.0 - fits.1) / (over.1 - fits.1) * (over.0 - fits.0);
                let r = used(s);
                if r <= 1.0 {
                    fits = (s, r);
                    if r > 0.985 {
                        break;
                    }
                } else {
                    over = (s, r);
                }
            }
            fits.0
        }
    };
    (s * FIT_MARGIN).max(MIN_SCALE)
}

/// The scene that shows the box with its labels, with `pad` px round them, in `frame` at every
/// angle a reader can turn it to. The box has one size whatever the view, so turning does not make
/// it grow or shrink: the largest size at which every angle the fit tries fits the frame, each
/// centred in it, so the box moves a little as its labels move from side to side and never leaves.
pub(super) fn fit(ctx: &Ctx, view: View, axes: &[Axis3; 3], frame: Rect, pad: f64) -> Scene {
    let key = fit_key(ctx, axes, frame, pad);
    let known = FITTED.with(|kept| kept.borrow().iter().find(|(k, _)| *k == key).map(|(_, s)| *s));
    let s = known.unwrap_or_else(|| {
        let s = size_for(ctx, axes, frame, pad);
        FITTED.with(|kept| {
            let mut kept = kept.borrow_mut();
            if kept.len() >= FITTED_KEPT {
                kept.remove(0);
            }
            kept.push((key, s));
        });
        s
    });
    let r = extent(ctx, view, axes, s).inflate(pad);
    let (centre, mid) = (frame.center(), r.center());
    Scene {
        view,
        s,
        c: [centre[0] - mid[0], centre[1] - mid[1]],
    }
}

/// `base` darkened towards the ink by `amount` (0 to 1).
pub(super) fn darker(ctx: &Ctx, base: Oklch, amount: f64) -> Oklch {
    color::mix(base, ctx.style.ink, amount)
}

/// `base` lightened towards the surface by `amount` (0 to 1).
pub(super) fn lighter(ctx: &Ctx, base: Oklch, amount: f64) -> Oklch {
    color::mix(base, ctx.style.surface, amount)
}

/// The colour of position `t` (0 to 1) along the style's ramp, from just after its start so the
/// lowest value is still visible on the surface.
pub(super) fn ramp_at(ctx: &Ctx, t: f64) -> Oklch {
    color::ramp(&ctx.style.ramp, 0.2 + 0.8 * t.clamp(0.0, 1.0))
}

/// A filled polygon through page points.
pub(super) fn polygon(points: &[Pt], fill: Fill, series: Option<usize>, obs: Option<usize>) -> Item {
    let mut path = Path::new();
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            path.move_to([p.x, p.y]);
        } else {
            path.line_to([p.x, p.y]);
        }
    }
    path.close();
    Item::Area {
        path,
        fill,
        role: Role::Mark,
        series,
        obs,
    }
}

/// The bounding box of page points.
pub(super) fn bounds_of(points: &[Pt]) -> Rect {
    let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for p in points {
        x0 = x0.min(p.x);
        x1 = x1.max(p.x);
        y0 = y0.min(p.y);
        y1 = y1.max(p.y);
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// A page point inside the polygon of `points`: their mean, or when a twisted quad puts the mean
/// outside, the mean of the first three corners or of the last three, whichever lies inside.
pub(super) fn centroid(points: &[Pt]) -> [f64; 2] {
    let mean = |ps: &[Pt]| {
        let n = ps.len().max(1) as f64;
        [
            ps.iter().map(|p| p.x).sum::<f64>() / n,
            ps.iter().map(|p| p.y).sum::<f64>() / n,
        ]
    };
    let inside = |at: [f64; 2]| {
        let mut odd = false;
        let mut j = points.len().wrapping_sub(1);
        for i in 0..points.len() {
            let (a, b) = (&points[i], &points[j]);
            if (a.y > at[1]) != (b.y > at[1]) && at[0] < (b.x - a.x) * (at[1] - a.y) / (b.y - a.y) + a.x {
                odd = !odd;
            }
            j = i;
        }
        odd
    };
    let all = mean(points);
    if points.len() < 4 || inside(all) {
        return all;
    }
    [mean(&points[..3]), mean(&points[1..])]
        .into_iter()
        .find(|&at| inside(at))
        .unwrap_or(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_higher_point_is_higher_on_the_page_and_a_nearer_one_is_nearer() {
        let v = View::new(0.0, 30.0);
        assert!(v.project([0.0, 0.0, 0.5]).y < v.project([0.0, 0.0, -0.5]).y);
        assert!(v.project([0.0, -0.5, 0.0]).depth > v.project([0.0, 0.5, 0.0]).depth);
        assert!(v.project([0.0, 0.5, 0.0]).y < v.project([0.0, -0.5, 0.0]).y);
    }

    #[test]
    fn the_azimuth_turns_the_cube_and_the_elevation_stays_inside_its_limits() {
        let side = View::new(90.0, 0.0);
        let p = side.project([0.5, 0.0, 0.0]);
        assert!(p.x.abs() < 1e-9, "the x axis points at the viewer");
        assert!(View::new(0.0, 200.0).project([0.0, 0.0, 1.0]).y < 0.0);
    }

    #[test]
    fn the_viewer_sees_the_faces_whose_normals_point_at_it() {
        let v = View::new(0.0, 30.0).toward_viewer();
        assert!(v[1] < 0.0 && v[2] > 0.0);
    }

    #[test]
    fn dragging_right_brings_the_near_side_right_and_dragging_down_brings_the_top_down() {
        let start = [35.0, 28.0];
        let from = View::new(start[0], start[1]);
        // A point on the floor's middle, towards the viewer, and the middle of the top.
        let toward = from.toward_viewer();
        let near = [toward[0] * 0.5, toward[1] * 0.5, 0.0];
        let top = [0.0, 0.0, 0.5];
        let right = turn_by_pixels(start, [30.0, 0.0]);
        let after = View::new(right[0], right[1]);
        assert!(
            after.project(near).x > from.project(near).x + 1e-6,
            "the near side follows the pointer right"
        );
        let left = turn_by_pixels(start, [-30.0, 0.0]);
        let after = View::new(left[0], left[1]);
        assert!(after.project(near).x < from.project(near).x - 1e-6, "and left");
        let down = turn_by_pixels(start, [0.0, 30.0]);
        let after = View::new(down[0], down[1]);
        assert!(
            after.project(top).y > from.project(top).y + 1e-6,
            "the top follows the pointer down"
        );
        let up = turn_by_pixels(start, [0.0, -30.0]);
        let after = View::new(up[0], up[1]);
        assert!(after.project(top).y < from.project(top).y - 1e-6, "and up");
    }

    #[test]
    fn a_view_is_wrapped_and_held_in_range_and_a_non_number_falls_back_to_the_defaults() {
        assert_eq!(clamp_view([725.0, 200.0]), [5.0, MAX_ELEVATION]);
        assert_eq!(clamp_view([-10.0, -3.0]), [350.0, MIN_ELEVATION]);
        assert_eq!(
            clamp_view([f64::NAN, f64::INFINITY]),
            [DEFAULT_AZIMUTH, DEFAULT_ELEVATION]
        );
        let held = turn_by_degrees([10.0, 84.0], [0.0, 30.0]);
        assert_eq!(held, [10.0, MAX_ELEVATION]);
        // Turning is a projection of the same cube at another angle: lengths along the vertical
        // axis keep their page length, whatever the azimuth.
        for a in [0.0, 77.0, 190.0] {
            let v = View::new(a, 40.0);
            let (p, q) = (v.project([0.1, 0.2, -0.5]), v.project([0.1, 0.2, 0.5]));
            assert!(((q.y - p.y).abs() - 40.0_f64.to_radians().cos()).abs() < 1e-9);
        }
    }

    #[test]
    fn a_glide_between_views_goes_the_short_way_round() {
        let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
        assert!(near(between_views([350.0, 20.0], [10.0, 40.0], 0.5), [0.0, 30.0]));
        assert!(near(between_views([10.0, 40.0], [350.0, 20.0], 0.5), [0.0, 30.0]));
        assert!(near(between_views([35.0, 28.0], [215.0, 60.0], 0.0), [35.0, 28.0]));
        let end = between_views([300.0, 80.0], [35.0, 28.0], 1.0);
        assert!(near(end, [35.0, 28.0]), "{end:?}");
        assert!(near(between_views([300.0, 80.0], [35.0, 28.0], f64::NAN), [35.0, 28.0]));
    }

    #[test]
    fn thinning_keeps_every_label_that_has_room() {
        assert_eq!(thin(4, 20.0, 400.0), 1);
        assert!(thin(10, 60.0, 200.0) > 1);
    }
}
