//! What the three-dimensional types share (`surface`, `scatter3d`, `bar3d`): an orthographic
//! projection of the unit cube onto the page, the box with its walls, grid and tick labels, and the
//! fit of the whole box with its labels into a frame. A painter maps its data into the cube
//! (each coordinate from -0.5 to 0.5), projects it with [`Scene::at`] and draws far marks first
//! (painter's algorithm), so the 3-D chart is plain 2-D items and every renderer and check works
//! on it unchanged (FEA-CH-75). The view comes from `style.azimuth` and `style.elevation`; turning
//! it with the pointer is host work that lays the chart out again with new angles.

use super::{Ctx, GRID_WIDTH, Style};
use crate::axis::{self, ValueSpec, Zero};
use crate::block::ChartBlock;
use crate::color::{self, Oklch};
use crate::geometry::{Anchor, DropReason, Face, Fill, Item, Path, Rect, Role, Stroke, Text};
use crate::numbers::Format;

/// Degrees the view turns round the vertical axis when the author gives none.
pub(super) const DEFAULT_AZIMUTH: f64 = 35.0;
/// Degrees the view looks down when the author gives none.
pub(super) const DEFAULT_ELEVATION: f64 = 28.0;
/// Gap between a box edge and its tick labels.
const TICK_GAP: f64 = 7.0;
/// Length of a tick mark.
const TICK_LEN: f64 = 4.0;

/// The direction the chart is seen from.
#[derive(Debug, Clone, Copy)]
pub(super) struct View {
    sin_a: f64,
    cos_a: f64,
    sin_e: f64,
    cos_e: f64,
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
        let a = azimuth.rem_euclid(360.0).to_radians();
        let e = elevation.clamp(5.0, 85.0).to_radians();
        View {
            sin_a: a.sin(),
            cos_a: a.cos(),
            sin_e: e.sin(),
            cos_e: e.cos(),
        }
    }

    /// The view the block asks for.
    pub fn of(block: &ChartBlock) -> View {
        let finite = |v: Option<f64>, default: f64| v.filter(|x| x.is_finite()).unwrap_or(default);
        View::new(
            finite(block.style_number("azimuth"), DEFAULT_AZIMUTH),
            finite(block.style_number("elevation"), DEFAULT_ELEVATION),
        )
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

/// The box as drawn: its items, their extent and the labels that did not fit.
pub(super) struct Boxed {
    pub items: Vec<Item>,
    pub bounds: Rect,
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
    let len = ex.hypot(ey);
    let mid = [(p.x + q.x) / 2.0 - scene.c[0], (p.y + q.y) / 2.0 - scene.c[1]];
    if len < 1e-9 {
        let m = mid[0].hypot(mid[1]);
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
    (q.x - p.x).hypot(q.y - p.y)
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

/// Draw the box of `axes` (x, y, z): grid on the floor and the two back walls, the edges, tick
/// labels along the two front floor edges and the left vertical edge, and the axis titles. With a
/// `frame`, labels that would leave it are dropped.
pub(super) fn draw_box(ctx: &Ctx, scene: &Scene, axes: &[Axis3; 3], frame: Option<Rect>) -> Boxed {
    let edge = Stroke::solid(Style::rgba(ctx.style.ink_faint), 1.0);
    let grid = Stroke::solid(Style::rgba(ctx.style.rule_soft), GRID_WIDTH);
    let ink = ctx.style.ink;
    let soft = ctx.style.ink_soft;
    let lh = ctx.line_height();
    // The floor corner nearest the viewer is the front; the walls stand behind it.
    let mut front = [-0.5, -0.5];
    let mut nearest = f64::NEG_INFINITY;
    for fx in [-0.5, 0.5] {
        for fy in [-0.5, 0.5] {
            let d = scene.view.project([fx, fy, -0.5]).depth;
            if d > nearest {
                nearest = d;
                front = [fx, fy];
            }
        }
    }
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
    let mut left = corners[0];
    let mut leftmost = f64::INFINITY;
    for c in corners {
        let x = scene.at([c[0], c[1], -0.5]).x;
        if x < leftmost - 1e-9 {
            leftmost = x;
            left = c;
        }
    }
    items.push(line(
        scene,
        [left[0], left[1], -0.5],
        [left[0], left[1], 0.5],
        &edge,
        Role::Baseline,
    ));

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
        let title = beside(
            ctx,
            &axis.title,
            [mid.x, mid.y],
            n,
            TICK_GAP + reach + 4.0,
            Role::AxisTitle,
            Face::Data,
            soft,
        );
        placer.put(title);
    }
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
    // Tick labels.
    let z_axis = &axes[2];
    for (t, label) in &z_axis.ticks {
        let p = scene.at([left[0], left[1], *t]);
        let text = ctx.text_mid(
            label,
            p.x - TICK_GAP,
            p.y,
            Anchor::End,
            Face::Figures,
            Role::AxisLabel,
            ink,
        );
        if placer.put(text) {
            tick_ink.push(Item::Line {
                path: Path::polyline(&[[p.x - TICK_LEN, p.y], [p.x, p.y]]),
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
    items.extend(tick_ink);
    items.extend(placer.items);
    let bounds = items
        .iter()
        .filter_map(Item::bounds)
        .reduce(|a, b| a.union(&b))
        .unwrap_or_default();
    Boxed {
        items,
        bounds,
        dropped: placer.dropped,
    }
}

/// The largest scene whose box and labels, with `pad` px round them, fit `frame`, centred in it.
pub(super) fn fit(ctx: &Ctx, view: View, axes: &[Axis3; 3], frame: Rect, pad: f64) -> Scene {
    let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for bx in [-0.5, 0.5] {
        for by in [-0.5, 0.5] {
            for bz in [-0.5, 0.5] {
                let p = view.project([bx, by, bz]);
                x0 = x0.min(p.x);
                x1 = x1.max(p.x);
                y0 = y0.min(p.y);
                y1 = y1.max(p.y);
            }
        }
    }
    let ceiling = (frame.w / (x1 - x0).max(1e-9)).min(frame.h / (y1 - y0).max(1e-9));
    let at = |s: f64| Scene { view, s, c: [0.0, 0.0] };
    let fits = |s: f64| {
        let b = draw_box(ctx, &at(s), axes, None).bounds.inflate(pad);
        b.w <= frame.w && b.h <= frame.h
    };
    let (mut lo, mut hi) = (ceiling.min(12.0), ceiling);
    if fits(hi) {
        lo = hi;
    } else {
        for _ in 0..16 {
            let mid = (lo + hi) / 2.0;
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
    }
    let b = draw_box(ctx, &at(lo), axes, None).bounds;
    let centre = frame.center();
    let mid = b.center();
    Scene {
        view,
        s: lo,
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
    fn thinning_keeps_every_label_that_has_room() {
        assert_eq!(thin(4, 20.0, 400.0), 1);
        assert!(thin(10, 60.0, 200.0) > 1);
    }
}
