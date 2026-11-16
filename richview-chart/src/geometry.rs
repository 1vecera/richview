//! The geometry model: one laid-out chart as plain data (ARC-18).
//!
//! The engine computes a [`Geometry`] from a validated chart block, its resolved rows, a width
//! and the style's chart tokens. Every renderer draws it and every check measures it: the report
//! runtime's WebGL renderer on the page, and [`crate::vector`] for PDF export. Nothing outside
//! the engine adds geometry of its own, so the page, the PDF and the checks cannot disagree.
//!
//! Coordinates are CSS pixels with the origin at the chart's top-left corner. Colours are
//! resolved [`Rgba`] values, so renderers need no style knowledge.

use serde::Serialize;
use std::rc::Rc;

/// An axis-aligned box in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub fn center(&self) -> [f64; 2] {
        [self.x + self.w / 2.0, self.y + self.h / 2.0]
    }
    /// Whether the two boxes overlap by more than `slack` pixels in both directions.
    pub fn overlaps(&self, other: &Rect, slack: f64) -> bool {
        self.x < other.right() - slack
            && other.x < self.right() - slack
            && self.y < other.bottom() - slack
            && other.y < self.bottom() - slack
    }
    /// Whether this box lies inside `outer`, allowing `slack` pixels.
    pub fn inside(&self, outer: &Rect, slack: f64) -> bool {
        self.x >= outer.x - slack
            && self.y >= outer.y - slack
            && self.right() <= outer.right() + slack
            && self.bottom() <= outer.bottom() + slack
    }
    pub fn contains(&self, [x, y]: [f64; 2]) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }
    /// The box grown by `by` pixels on every side.
    pub fn inflate(&self, by: f64) -> Rect {
        Rect::new(self.x - by, self.y - by, self.w + 2.0 * by, self.h + 2.0 * by)
    }
    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }
    /// Never negative: a degenerate box keeps its origin with zero extent (FEA-BR-19).
    pub fn clamped(self) -> Rect {
        Rect::new(self.x, self.y, self.w.max(0.0), self.h.max(0.0))
    }
}

/// A resolved colour: gamma-encoded sRGB channels and alpha, each 0–1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rgba(pub [f32; 4]);

impl Rgba {
    pub fn from_oklch(color: crate::color::Oklch, alpha: f64) -> Self {
        let [r, g, b] = color.srgb();
        Rgba([r as f32, g as f32, b as f32, alpha.clamp(0.0, 1.0) as f32])
    }
    pub fn with_alpha(self, alpha: f64) -> Self {
        let [r, g, b, _] = self.0;
        Rgba([r, g, b, alpha.clamp(0.0, 1.0) as f32])
    }
    pub fn alpha(self) -> f64 {
        self.0[3] as f64
    }
    /// `#rrggbb` for vector output; alpha travels separately as an opacity attribute.
    pub fn hex(self) -> String {
        let [r, g, b, _] = self.0.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        format!("#{r:02x}{g:02x}{b:02x}")
    }
    pub fn oklch(self) -> crate::color::Oklch {
        let [r, g, b, _] = self.0;
        crate::color::Oklch::from_srgb([r as f64, g as f64, b as f64])
    }
}

/// One drawing command of a [`Path`]. Curves stay cubic Béziers in the model; the WebGL
/// renderer flattens them at the device's resolution and the vector renderer writes them as is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "c", rename_all = "lowercase")]
pub enum Cmd {
    M { p: [f64; 2] },
    L { p: [f64; 2] },
    C { c1: [f64; 2], c2: [f64; 2], p: [f64; 2] },
    Z,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Path {
    pub cmds: Vec<Cmd>,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn move_to(&mut self, p: [f64; 2]) {
        self.cmds.push(Cmd::M { p });
    }
    pub fn line_to(&mut self, p: [f64; 2]) {
        self.cmds.push(Cmd::L { p });
    }
    pub fn curve_to(&mut self, c1: [f64; 2], c2: [f64; 2], p: [f64; 2]) {
        self.cmds.push(Cmd::C { c1, c2, p });
    }
    pub fn close(&mut self) {
        self.cmds.push(Cmd::Z);
    }
    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }
    /// A polyline through `points`.
    pub fn polyline(points: &[[f64; 2]]) -> Self {
        let mut path = Path::new();
        for (i, p) in points.iter().enumerate() {
            if i == 0 { path.move_to(*p) } else { path.line_to(*p) }
        }
        path
    }
    /// Move every point by `dx`, `dy`.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        let shift = |p: &mut [f64; 2]| {
            p[0] += dx;
            p[1] += dy;
        };
        for cmd in &mut self.cmds {
            match cmd {
                Cmd::M { p } | Cmd::L { p } => shift(p),
                Cmd::C { c1, c2, p } => {
                    shift(c1);
                    shift(c2);
                    shift(p);
                }
                Cmd::Z => {}
            }
        }
    }
    /// Append another path's commands.
    pub fn extend(&mut self, other: &Path) {
        self.cmds.extend_from_slice(&other.cmds);
    }
    /// Every point the path passes through or pulls towards: a conservative bounding box.
    pub fn bounds(&self) -> Option<Rect> {
        let mut points = self.cmds.iter().flat_map(|c| match c {
            Cmd::M { p } | Cmd::L { p } => vec![*p],
            Cmd::C { c1, c2, p } => vec![*c1, *c2, *p],
            Cmd::Z => vec![],
        });
        let first = points.next()?;
        let (mut x0, mut y0, mut x1, mut y1) = (first[0], first[1], first[0], first[1]);
        for [x, y] in points {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
    }
    /// The path flattened to polylines (one per subpath) within `tolerance` pixels.
    pub fn flatten(&self, tolerance: f64) -> Vec<Vec<[f64; 2]>> {
        let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
        let mut start = [0.0, 0.0];
        let mut at = [0.0, 0.0];
        for cmd in &self.cmds {
            match *cmd {
                Cmd::M { p } => {
                    out.push(vec![p]);
                    start = p;
                    at = p;
                }
                Cmd::L { p } => {
                    if out.is_empty() {
                        out.push(vec![at]);
                    }
                    out.last_mut().expect("subpath").push(p);
                    at = p;
                }
                Cmd::C { c1, c2, p } => {
                    if out.is_empty() {
                        out.push(vec![at]);
                    }
                    let last = out.last_mut().expect("subpath");
                    flatten_cubic([at, c1, c2, p], tolerance.max(0.05), 0, last);
                    at = p;
                }
                Cmd::Z => {
                    if let Some(last) = out.last_mut()
                        && last.first() != last.last()
                    {
                        last.push(start);
                    }
                    at = start;
                }
            }
        }
        out
    }
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Distance to the closed chord: collinear handles beyond its ends still need subdivision.
fn chord_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let length = dist(a, b);
    if length == 0.0 {
        return dist(p, a);
    }
    let unit = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
    let along = ((p[0] - a[0]) * unit[0] + (p[1] - a[1]) * unit[1]).clamp(0.0, length);
    dist(p, [a[0] + unit[0] * along, a[1] + unit[1] * along])
}

/// Split only while the control polygon can leave the chord's tolerance. Every command's
/// endpoint is appended, including coincident observations; this changes no data selection.
fn flatten_cubic(points: [[f64; 2]; 4], tolerance: f64, depth: u8, out: &mut Vec<[f64; 2]>) {
    let [a, b, c, d] = points;
    if depth == 16 || chord_distance(b, a, d).max(chord_distance(c, a, d)) <= tolerance {
        out.push(d);
        return;
    }
    let midpoint = |a: [f64; 2], b: [f64; 2]| [a[0] * 0.5 + b[0] * 0.5, a[1] * 0.5 + b[1] * 0.5];
    let (ab, bc, cd) = (midpoint(a, b), midpoint(b, c), midpoint(c, d));
    let (abc, bcd) = (midpoint(ab, bc), midpoint(bc, cd));
    let middle = midpoint(abc, bcd);
    flatten_cubic([a, ab, abc, middle], tolerance, depth + 1, out);
    flatten_cubic([middle, bcd, cd, d], tolerance, depth + 1, out);
}

#[cfg(test)]
fn cubic(p0: [f64; 2], p1: [f64; 2], p2: [f64; 2], p3: [f64; 2], t: f64) -> [f64; 2] {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
        a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
    ]
}

/// A hatch pattern drawn over a fill when colour alone cannot tell series apart (FEA-CH-57).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Hatch {
    Diagonal,
    Back,
    Horizontal,
    Vertical,
    Cross,
    Dots,
}

impl Hatch {
    pub const ALL: [Hatch; 6] = [
        Hatch::Diagonal,
        Hatch::Back,
        Hatch::Horizontal,
        Hatch::Vertical,
        Hatch::Cross,
        Hatch::Dots,
    ];
    pub fn for_slot(slot: usize) -> Hatch {
        Hatch::ALL[slot % Hatch::ALL.len()]
    }
}

/// A hollow marker shape for line and point series when decals are on (FEA-CH-57).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    Circle,
    Square,
    Triangle,
    Diamond,
    Plus,
    Cross,
}

impl Shape {
    pub const ALL: [Shape; 6] = [
        Shape::Circle,
        Shape::Square,
        Shape::Triangle,
        Shape::Diamond,
        Shape::Plus,
        Shape::Cross,
    ];
    pub fn for_slot(slot: usize) -> Shape {
        Shape::ALL[slot % Shape::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Fill {
    pub color: Rgba,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hatch: Option<Hatch>,
    /// Ink for the hatch lines.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hatch_ink: Option<Rgba>,
    /// Multiplier on the hatch's 6 px pitch: 1 for a large mark, more for a small one so its
    /// hatch never closes up into a solid.
    #[serde(skip_serializing_if = "is_one")]
    pub hatch_scale: f64,
}

fn is_one(v: &f64) -> bool {
    *v == 1.0
}

impl Fill {
    pub fn solid(color: Rgba) -> Self {
        Self {
            color,
            hatch: None,
            hatch_ink: None,
            hatch_scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Cap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stroke {
    pub color: Rgba,
    pub width: f64,
    pub cap: Cap,
    /// Dash and gap lengths in pixels; empty for a solid line.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dash: Vec<f64>,
}

impl Stroke {
    pub fn solid(color: Rgba, width: f64) -> Self {
        Self {
            color,
            width,
            cap: Cap::Butt,
            dash: Vec::new(),
        }
    }
}

/// What a drawn item is, so interaction and motion can address it without new geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Grid,
    Baseline,
    Tick,
    Leader,
    Mark,
    Band,
    Missing,
    Rug,
    Annotation,
    Label,
    AxisLabel,
    AxisTitle,
    PanelTitle,
    Key,
    Notice,
}

/// Which typeface a text item uses; the style resolves it to a bundled family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Face {
    /// Ticks, axis titles, data and end labels, readout values (STY-39).
    Data,
    /// Numbers set in the data face's tabular variant where the style has one (OPN-62).
    Figures,
    Ui,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// One line of chart text, laid out with the face's real advance widths (FEA-CH-30).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Text {
    pub text: String,
    /// Anchor point: `x` per `anchor`, `y` the alphabetic baseline.
    pub x: f64,
    pub y: f64,
    pub anchor: Anchor,
    pub face: Face,
    pub size: f64,
    pub weight: u16,
    pub color: Rgba,
    /// Rotation in degrees around the anchor point, clockwise.
    #[serde(skip_serializing_if = "is_zero")]
    pub rotate: f64,
    /// The laid-out box of the unrotated text: advance width by ascent plus descent.
    pub bounds: Rect,
    pub role: Role,
    /// The series this text belongs to, for emphasis; only genuine outside-plot line and area
    /// names become series buttons (FEA-CH-27).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<usize>,
    /// The id of the mark this value text describes (see [`Geometry::mark_ids`]); `None` for a
    /// text that is no single mark's (a total, a key value, an annotation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mark: Option<String>,
    /// Whether `mark` was found by nearness ([`Geometry::attribute_marks`]) and not set by the
    /// text's painter. Two layouts of one chart can find different marks for such a text, so it
    /// never shares a slot with a text whose painter named its mark. Not part of the model a
    /// page reads.
    #[serde(skip)]
    pub near: bool,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Item {
    Rect {
        rect: Rect,
        fill: Fill,
        #[serde(skip_serializing_if = "Option::is_none")]
        stroke: Option<Stroke>,
        /// Corner radius applied only to the value end of a bar: `[top-left, top-right,
        /// bottom-right, bottom-left]`.
        radius: [f64; 4],
        role: Role,
        #[serde(skip_serializing_if = "Option::is_none")]
        series: Option<usize>,
        /// Observation index for readouts and motion stagger.
        #[serde(skip_serializing_if = "Option::is_none")]
        obs: Option<usize>,
    },
    Line {
        path: Path,
        stroke: Stroke,
        role: Role,
        #[serde(skip_serializing_if = "Option::is_none")]
        series: Option<usize>,
        /// Observation whose mark this line belongs to, such as an OHLC wick or tick.
        #[serde(skip_serializing_if = "Option::is_none")]
        obs: Option<usize>,
    },
    Area {
        path: Path,
        fill: Fill,
        role: Role,
        #[serde(skip_serializing_if = "Option::is_none")]
        series: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        obs: Option<usize>,
    },
    Point {
        center: [f64; 2],
        radius: f64,
        shape: Shape,
        #[serde(skip_serializing_if = "Option::is_none")]
        fill: Option<Fill>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stroke: Option<Stroke>,
        role: Role,
        #[serde(skip_serializing_if = "Option::is_none")]
        series: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        obs: Option<usize>,
    },
    Text(Text),
}

impl Item {
    pub fn role(&self) -> Role {
        match self {
            Item::Rect { role, .. } | Item::Line { role, .. } | Item::Area { role, .. } | Item::Point { role, .. } => {
                *role
            }
            Item::Text(t) => t.role,
        }
    }
    pub fn series(&self) -> Option<usize> {
        match self {
            Item::Rect { series, .. }
            | Item::Line { series, .. }
            | Item::Area { series, .. }
            | Item::Point { series, .. } => *series,
            Item::Text(t) => t.series,
        }
    }
    /// The painted extent, including half the stroke width.
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Item::Rect { rect, stroke, .. } => Some(rect.inflate(stroke.as_ref().map_or(0.0, |s| s.width / 2.0))),
            Item::Line { path, stroke, .. } => path.bounds().map(|r| r.inflate(stroke.width / 2.0)),
            Item::Area { path, .. } => path.bounds(),
            Item::Point {
                center, radius, stroke, ..
            } => Some(
                Rect::new(center[0] - radius, center[1] - radius, 2.0 * radius, 2.0 * radius)
                    .inflate(stroke.as_ref().map_or(0.0, |s| s.width / 2.0)),
            ),
            Item::Text(t) => Some(t.bounds),
        }
    }
}

/// A series or category the reader can tell apart: its colour slot and cues.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Series {
    pub key: String,
    pub name: String,
    /// Palette slot, stable across facets, hidden series and source updates (FEA-CH-61, FEA-CH-63).
    pub slot: usize,
    pub color: Rgba,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hatch: Option<Hatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<Shape>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dash: Vec<f64>,
    pub hidden: bool,
}

/// How the legend draws a series swatch (STY-40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Swatch {
    Square,
    Line,
    Dot,
}

/// The legend, when series are not named directly (FEA-CH-33). The report sets it in HTML above
/// or below the canvas, as the style places it, and the runtime makes its entries buttons.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Legend {
    pub swatch: Swatch,
    /// Series indices in legend order.
    pub entries: Vec<usize>,
    pub bottom: bool,
}

/// One observation the reader can inspect: where it sits and what a readout says about it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Observation {
    /// Index into the chart's source rows; `None` for derived values (bins, quartiles).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<usize>,
    /// Hover dots for every series this observation lists (lines and areas).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dots: Vec<Dot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<usize>,
    /// Anchor for the hover dot and readout.
    pub at: [f64; 2],
    /// Filled marks are hit by containment; points by radius.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<Rect>,
    /// Pie slices are hit by angle and radius.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wedge: Option<Wedge>,
    pub radius: f64,
    /// Position along x for nearest-x targeting on lines and areas.
    pub x: f64,
    /// Shared with the preparation that formatted it, so a zoom or resize frame copies no
    /// readout text (PRF-19).
    pub readout: Rc<Readout>,
}

/// A pie or ring slice for hit testing: angles in radians clockwise from 12 o'clock.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Wedge {
    pub center: [f64; 2],
    pub radius: f64,
    /// Radius of the hole of a ring slice; 0 for a pie slice.
    pub inner: f64,
    pub start: f64,
    pub end: f64,
}

/// A hover dot on one series at the targeted x.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Dot {
    pub at: [f64; 2],
    pub series: usize,
}

/// What a readout shows: a heading and named values (FEA-CH-40).
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Readout {
    pub heading: String,
    pub lines: Vec<ReadoutLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReadoutLine {
    pub name: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<usize>,
    /// True when this row names the series identified by `series`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub names_series: bool,
    /// True for values RichView computed (bins, quartiles, totals).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub derived: bool,
}

/// How a panel's observations are targeted (FEA-CH-41).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Targeting {
    /// Every visible series at the nearest x anywhere over the plot. The panel's observations
    /// are in x order, left to right or right to left, so hover bisects them (PRF-19).
    NearestX,
    /// The nearest point within its radius or 24 px for a pointer, 32 px for touch.
    Radius,
    /// The filled mark containing the pointer.
    Contain,
}

/// The x axis as the runtime needs it for zoom and pan (FEA-CH-49).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct XScale {
    /// Data-space domain of the visible window.
    pub domain: [f64; 2],
    /// The whole data extent, the zoom limit.
    pub extent: [f64; 2],
    /// Pixel range across the plot.
    pub range: [f64; 2],
    pub calendar: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Panel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet: Option<String>,
    /// The panel's whole box, including axes and its title.
    pub frame: Rect,
    /// The plotting area, where marks live and clipping is measured.
    pub plot: Rect,
    /// Items in paint order.
    pub items: Vec<Item>,
    pub observations: Vec<Observation>,
    pub targeting: Targeting,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<XScale>,
    /// A panel that cannot draw shows this reason in its plot (FEA-CH-18).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
}

/// A check the engine ran on its own geometry (T-CHART-CHECK).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Check {
    pub kind: String,
    pub level: crate::diag::Level,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet: Option<String>,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
}

/// A label the placement search could not draw (T-DROPPED-LABEL).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dropped {
    pub text: String,
    pub reason: DropReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub would_be_at: Option<Rect>,
    /// The panel and observation that produced an automatic value drop in this geometry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation: Option<[usize; 2]>,
    /// The observation's primary readout line, including a recorded density evaluation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<usize>,
    /// The observation's readout line this value is, when its mark prints more than one value of
    /// a series and this is not the first: a dumbbell's upper end. It names the line whose
    /// canonical text [`Geometry::restore_value_drop_text`] gives back; it is no part of the
    /// model's JSON.
    #[serde(skip)]
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DropReason {
    Collision,
    OutOfBounds,
    Density,
}

/// The entrance a chart plays once (FEA-CH-51); the runtime steps it, the model holds no frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Entrance {
    /// Lines and areas sweep in from the left over 700 ms: strokes end at the moving front,
    /// fills fade in over the 48 px behind it.
    Sweep,
    /// Bars grow from the baseline over 500 ms, 40 ms apart within a 320 ms spread. Marks
    /// without a baseline (tiles, cells, boxes) grow from their own centre in a wave from the
    /// plot's top left.
    Grow,
    /// Pie slices fan out over 700 ms.
    Fan,
    /// Points settle over 400 ms: they fade in from half their radius.
    Settle,
    /// Shapes that neither grow nor sweep, and any chart from 2,000 marks, fade in over 300 ms.
    Fade,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Geometry {
    pub kind: String,
    pub width: f64,
    pub height: f64,
    pub background: Rgba,
    pub series: Vec<Series>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<Legend>,
    pub panels: Vec<Panel>,
    /// Items drawn once for the whole figure: keys under the plot, the size reference.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub figure_items: Vec<Item>,
    /// Plain-language method note under the plot (FEA-CH-14), set in HTML by the report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// One-sentence summary for the figure's accessible name (FEA-CH-55).
    pub summary: String,
    pub checks: Vec<Check>,
    pub dropped: Vec<Dropped>,
    /// `None` when the style turns motion off (STY-40).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrance: Option<Entrance>,
    /// The style's entrance timing, which the runtime plays `entrance` with (INT-12).
    #[serde(skip)]
    pub timing: crate::motion::Profile,
    /// The whole chart cannot be drawn; its frame shows this reason (FEA-CH-18).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    /// The data extents the scales cover, for a playback to fit every frame (FEA-BR-31).
    #[serde(skip)]
    pub fit: crate::layout::Fit,
    /// What the layout derived from the exact values, for a layout from grid rows to reuse
    /// (TYP-36).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub derived: Vec<crate::derived::Derived>,
}

impl Geometry {
    /// Every item in paint order, figure items last.
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.panels
            .iter()
            .flat_map(|p| p.items.iter())
            .chain(self.figure_items.iter())
    }
    pub fn mark_count(&self) -> usize {
        self.items().filter(|i| i.role() == Role::Mark).count()
    }

    /// Gives every value text the id of the mark it stands at: the nearest observation of its
    /// panel by box, within reach of 80 px (ties to the earlier), among the observations of its
    /// own series when it names one. A text with no observation in reach keeps no mark. Painters
    /// that know their mark set `Text::mark` themselves; those texts are left as they are.
    pub fn attribute_marks(&mut self) {
        for p in 0..self.panels.len() {
            for i in 0..self.panels[p].items.len() {
                let panel = &self.panels[p];
                if let Item::Text(t) = &panel.items[i]
                    && t.mark.is_none()
                    && t.is_value_text()
                    && let Some(k) = nearest_observation(t, &panel.observations)
                {
                    let id = self.mark_id(p, k);
                    if let Item::Text(t) = &mut self.panels[p].items[i] {
                        t.mark = Some(id);
                        t.near = true;
                    }
                }
            }
        }
    }

    /// The value texts keyed by the mark they describe and their ordinal among that mark's texts
    /// in paint order. A text with no mark is keyed `<panel>/text/<ordinal>` (`figure/text/...`
    /// outside the panels), so a hidden series or a zoom never shifts another mark's text (TYP-36).
    /// A text marked by nearness is keyed `near/<mark>/<ordinal>`, apart from the texts whose
    /// painter named that mark: a line-end name that is nearest to one observation in the grid
    /// layout and to its neighbour in the shown-value layout cannot move a value onto another.
    pub fn mark_texts(&self) -> Vec<MarkText> {
        let mut out = Vec::new();
        for (scope, items) in self.text_scopes() {
            let mut slots: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
            for t in items.filter_map(|i| match i {
                Item::Text(t) if t.is_value_text() => Some(t),
                _ => None,
            }) {
                let id = t.mark_key(&scope);
                let slot = slots.entry(id.clone()).or_insert(0);
                out.push(MarkText {
                    id,
                    slot: *slot,
                    text: t.text.clone(),
                });
                *slot += 1;
            }
        }
        out
    }

    fn text_scopes(&self) -> Vec<(String, std::slice::Iter<'_, Item>)> {
        let mut scopes: Vec<(String, std::slice::Iter<'_, Item>)> = self
            .panels
            .iter()
            .enumerate()
            .map(|(p, panel)| (p.to_string(), panel.items.iter()))
            .collect();
        scopes.push(("figure".to_owned(), self.figure_items.iter()));
        scopes
    }

    /// Replaces the text of every value text that `texts` has a text for, by mark id and slot, and
    /// returns whether every value text of the figure was replaced. A text with no counterpart
    /// keeps what the figure has.
    pub fn set_mark_texts(&mut self, texts: &[MarkText]) -> bool {
        let by_key: std::collections::HashMap<(&str, u32), &str> = texts
            .iter()
            .map(|t| ((t.id.as_str(), t.slot), t.text.as_str()))
            .collect();
        let mut all = true;
        let panels = self
            .panels
            .iter_mut()
            .enumerate()
            .map(|(p, panel)| (p.to_string(), &mut panel.items));
        let figure = std::iter::once(("figure".to_owned(), &mut self.figure_items));
        for (scope, items) in panels.chain(figure) {
            let mut slots: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
            for item in items.iter_mut() {
                let Item::Text(t) = item else { continue };
                if !t.is_value_text() {
                    continue;
                }
                let id = t.mark_key(&scope);
                let slot = slots.entry(id.clone()).or_insert(0);
                match by_key.get(&(id.as_str(), *slot)) {
                    Some(text) => t.replace_text(text),
                    None => all = false,
                }
                *slot += 1;
            }
        }
        all
    }

    /// Restores dropped values and their diagnostics from the canonical observation readouts: a
    /// value takes the text of the readout line it names, else of the first line of its series.
    pub fn restore_value_drop_text(&mut self) {
        let mut changed = false;
        for dropped in &mut self.dropped {
            if let Some([panel, observation]) = dropped.observation
                && let Some(series) = dropped.series
                && let Some(readout) = self
                    .panels
                    .get(panel)
                    .and_then(|p| p.observations.get(observation))
                    .map(|o| &o.readout)
                && let Some(line) = dropped
                    .line
                    .and_then(|n| readout.lines.get(n))
                    .filter(|line| line.series == Some(series))
                    .or_else(|| readout.lines.iter().find(|line| line.series == Some(series)))
                && dropped.text != line.value
            {
                dropped.text.clone_from(&line.value);
                changed = true;
            }
        }
        if changed {
            self.checks.retain(|check| check.kind != "label-dropped");
            self.checks.extend(crate::layout::dropped_checks(&self.dropped));
        }
    }

    /// A stable id for each observation, in observation order: `row:<i>` for a mark that is one
    /// source row, else `<panel>/<series>/<ordinal>` counted within its panel and series, so a
    /// hidden series or another mark's text never shifts it (TYP-36).
    pub fn mark_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for (p, panel) in self.panels.iter().enumerate() {
            let mut counts: std::collections::BTreeMap<Option<usize>, usize> = std::collections::BTreeMap::new();
            for o in &panel.observations {
                let n = counts.entry(o.series).or_insert(0);
                ids.push(match o.row {
                    Some(row) => format!("row:{row}"),
                    None => match o.series {
                        Some(s) => format!("{p}/{s}/{n}"),
                        None => format!("{p}/-/{n}"),
                    },
                });
                *n += 1;
            }
        }
        ids
    }

    /// The id [`Geometry::mark_ids`] gives observation `k` of panel `p`, without the others' ids:
    /// a value text needs only the id of the mark it stands at.
    fn mark_id(&self, p: usize, k: usize) -> String {
        let observations = &self.panels[p].observations;
        let o = &observations[k];
        let n = observations[..k].iter().filter(|q| q.series == o.series).count();
        match (o.row, o.series) {
            (Some(row), _) => format!("row:{row}"),
            (None, Some(s)) => format!("{p}/{s}/{n}"),
            (None, None) => format!("{p}/-/{n}"),
        }
    }
}

/// The text of one on-chart value label, keyed by the mark it prints and its ordinal among that
/// mark's texts (see [`Geometry::mark_texts`]).
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize, Default)]
pub struct MarkText {
    pub id: String,
    #[serde(skip_serializing_if = "is_zero_slot", default)]
    pub slot: u32,
    pub text: String,
}

fn is_zero_slot(slot: &u32) -> bool {
    *slot == 0
}

/// The index of the observation a text stands at: the nearest by its box to the observation's
/// area, or to its anchor, within reach of 80 px; ties go to the earlier observation.
fn nearest_observation(text: &Text, observations: &[Observation]) -> Option<usize> {
    let c = [text.bounds.x + text.bounds.w / 2.0, text.bounds.y + text.bounds.h / 2.0];
    let mut best: Option<(usize, f64)> = None;
    for (k, o) in observations.iter().enumerate() {
        if text.series.is_some() && o.series.is_some() && text.series != o.series {
            continue;
        }
        let d = match o.area {
            Some(a) => {
                let dx = (a.x - c[0]).max(0.0).max(c[0] - a.right());
                let dy = (a.y - c[1]).max(0.0).max(c[1] - a.bottom());
                dx.hypot(dy)
            }
            None => (o.at[0] - c[0]).hypot(o.at[1] - c[1]),
        };
        if d < 80.0 && best.is_none_or(|(_, b)| d < b - 1e-9) {
            best = Some((k, d));
        }
    }
    best.map(|(k, _)| k)
}

impl Text {
    /// Sets the text and fits its box to it: the width follows the number of characters (the
    /// numbers this is used for are set in a face whose digits share a width), kept on the same
    /// anchor.
    fn replace_text(&mut self, text: &str) {
        let (old, new) = (self.text.chars().count().max(1), text.chars().count());
        if old != new && self.bounds.w > 0.0 {
            let width = self.bounds.w * new as f64 / old as f64;
            self.bounds.x = match self.anchor {
                Anchor::Start => self.bounds.x,
                Anchor::Middle => self.x - width / 2.0,
                Anchor::End => self.x - width,
            };
            self.bounds.w = width;
        }
        self.text = text.to_owned();
    }

    /// The key of this text's slots in [`Geometry::mark_texts`], in the panel or figure `scope`.
    fn mark_key(&self, scope: &str) -> String {
        match &self.mark {
            Some(id) if self.near => format!("near/{id}"),
            Some(id) => id.clone(),
            None => format!("{scope}/text"),
        }
    }

    /// Whether the text prints a number the data decides.
    pub fn is_value_text(&self) -> bool {
        matches!(self.role, Role::Label | Role::Annotation | Role::Key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattening_keeps_end_points_and_closes_subpaths() {
        let mut path = Path::new();
        path.move_to([0.0, 0.0]);
        path.curve_to([10.0, 0.0], [10.0, 10.0], [20.0, 10.0]);
        path.line_to([20.0, 20.0]);
        path.close();
        let lines = path.flatten(0.25);
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        assert_eq!(line.first(), Some(&[0.0, 0.0]));
        assert_eq!(line.last(), Some(&[0.0, 0.0]));
        assert!(line.contains(&[20.0, 10.0]) && line.contains(&[20.0, 20.0]));
        assert!(line.len() > 4, "the curve is subdivided");
    }

    #[test]
    fn nearly_straight_cubics_retain_each_endpoint_without_length_based_oversampling() {
        let mut path = Path::new();
        path.move_to([0.0, 0.0]);
        path.curve_to([0.001, 20.0], [-0.001, 80.0], [0.0, 100.0]);
        path.curve_to([0.001, 120.0], [-0.001, 180.0], [0.0, 200.0]);
        assert_eq!(path.flatten(0.25)[0], [[0.0, 0.0], [0.0, 100.0], [0.0, 200.0]]);
    }

    #[test]
    fn adaptive_cubics_stay_within_pixel_tolerance_even_for_loops_and_collinear_overshoots() {
        for points in [
            [[0.0, 0.0], [100.0, 300.0], [-100.0, 300.0], [0.0, 0.0]],
            [[0.0, 0.0], [500.0, 0.0], [-400.0, 0.0], [100.0, 0.0]],
            [[0.0, 0.0], [0.0, 400.0], [400.0, -400.0], [400.0, 0.0]],
        ] {
            let mut path = Path::new();
            path.move_to(points[0]);
            path.curve_to(points[1], points[2], points[3]);
            let line = &path.flatten(0.25)[0];
            assert_eq!(line.first(), Some(&points[0]));
            assert_eq!(line.last(), Some(&points[3]));
            for i in 0..=4_000 {
                let p = cubic(points[0], points[1], points[2], points[3], i as f64 / 4_000.0);
                let error = line
                    .windows(2)
                    .map(|w| chord_distance(p, w[0], w[1]))
                    .fold(f64::INFINITY, f64::min);
                assert!(error <= 0.250001, "curve {points:?} at {i} misses by {error}px");
            }
        }
    }

    #[test]
    fn mark_texts_key_each_value_text_by_its_mark_and_swap_in_place() {
        let block = crate::block::ChartBlock::from_value(&serde_json::json!({
            "type": "funnel", "title": "T", "encoding": {"x": "c", "y": "v"}
        }))
        .expect("block");
        let rows: Vec<serde_json::Value> = (0..4)
            .map(|i| serde_json::json!({"c": format!("c{i}"), "v": 3 + i * 5}))
            .collect();
        let mut g = crate::layout::layout(
            &block,
            &rows,
            &crate::layout::ChartTheme::default(),
            &crate::layout::Options::default(),
        );
        let texts = g.mark_texts();
        assert_eq!(texts, g.mark_texts());
        let mut keys: Vec<(&str, u32)> = texts.iter().map(|t| (t.id.as_str(), t.slot)).collect();
        let before = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), before, "every key is unique: {texts:?}");
        let marked: Vec<MarkText> = texts
            .iter()
            .map(|t| MarkText {
                text: format!("{}!", t.text),
                ..t.clone()
            })
            .collect();
        assert!(g.set_mark_texts(&marked));
        assert_eq!(g.mark_texts(), marked);
        assert!(!g.set_mark_texts(&marked[1..]));
    }

    #[test]
    fn a_name_near_another_mark_in_the_other_layout_leaves_each_value_its_own_text() {
        // The runtime lays a chart out over grid values and over shown values and copies the
        // shown texts across. A name beside the marks can sit a pixel apart in the two and be
        // nearest to one mark here and to its neighbour there (a line-end name beside the last
        // two observations of its line).
        let block = crate::block::ChartBlock::from_value(&serde_json::json!({
            "type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}
        }))
        .expect("block");
        let rows: Vec<serde_json::Value> = (0..3)
            .map(|i| serde_json::json!({"c": format!("c{i}"), "v": 30 + i * 5}))
            .collect();
        let mut shown = crate::layout::layout(
            &block,
            &rows,
            &crate::layout::ChartTheme::default(),
            &crate::layout::Options::default(),
        );
        let values = |g: &Geometry| -> Vec<(String, String)> {
            g.items()
                .filter_map(|i| match i {
                    Item::Text(t) if t.is_value_text() && !t.near => Some((t.mark.clone()?, t.text.clone())),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            values(&shown),
            [("row:0", "30"), ("row:1", "35"), ("row:2", "40")].map(|(id, text)| (id.to_owned(), text.to_owned())),
            "each bar's value names its own row"
        );
        let name = |g: &Geometry, mark: &str| {
            let value = g.items().find_map(|i| match i {
                Item::Text(t) if t.is_value_text() => Some(t.clone()),
                _ => None,
            });
            Item::Text(Text {
                text: "Name".to_owned(),
                mark: Some(mark.to_owned()),
                near: true,
                ..value.expect("a value text")
            })
        };
        let mut grid = shown.clone();
        let (here, there) = (name(&grid, "row:0"), name(&shown, "row:1"));
        grid.panels[0].items.insert(0, here);
        shown.panels[0].items.insert(0, there);
        grid.set_mark_texts(&shown.mark_texts());
        assert_eq!(
            values(&grid),
            values(&shown),
            "no value takes the name or another value"
        );
        assert!(
            matches!(&grid.panels[0].items[0], Item::Text(t) if t.text == "Name"),
            "the name keeps its own text"
        );
    }

    #[test]
    fn bounds_cover_control_points_and_strokes() {
        let item = Item::Line {
            path: Path::polyline(&[[10.0, 10.0], [30.0, 50.0]]),
            stroke: Stroke::solid(Rgba([0.0, 0.0, 0.0, 1.0]), 2.0),
            role: Role::Mark,
            series: Some(0),
            obs: None,
        };
        assert_eq!(item.bounds(), Some(Rect::new(9.0, 9.0, 22.0, 42.0)));
    }
}
