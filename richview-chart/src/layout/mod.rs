//! From a validated chart block, its rows, a width and the style's chart theme to one
//! [`Geometry`] (ARC-18). One painter per family shares this frame: series identity and colour
//! slots, facets, text measurement, the placement search and the checks.

mod alluvial;
mod band;
mod bar3d;
mod bin2d;
mod bullet;
mod calendar;
mod cartesian;
mod checks;
pub(crate) use checks::dropped_checks;
mod chord;
mod combo;
mod contour;
mod density;
mod dotplot;
mod dumbbell;
mod ecdf;
mod financial;
mod finish;
mod flow;
mod funnel;
mod gantt;
mod gauge;
mod graph;
mod heatmap;
mod hexbin;
mod hierarchy;
mod icicle;
mod kit;
mod lollipop;
mod map;
mod network;
mod packed_bubble;
mod parallel;
mod pareto;
mod pie;
mod pivot;
mod place;
mod project3d;
mod qq;
mod radar;
mod ribbon;
mod rose;
mod sample;
mod sankey;
mod scatter3d;
mod splom;
mod stats;
mod streamgraph;
mod strip;
mod sunburst;
mod surface;
mod tree;
mod treemap;
pub(crate) mod types;
mod vector;
mod venn;
mod violin;
mod waffle;
mod waterfall;
mod wordcloud;

use crate::block::{ChartBlock, ChartType, Curve, PaletteRef};
use crate::color::{self, Oklch};
use crate::diag::Level;
use crate::geometry::{
    Anchor, Check, DropReason, Dropped, Entrance, Face, Geometry, Hatch, Observation, Panel, Rect, Rgba, Role, Series,
    Shape, Text,
};
use crate::numbers::Locale;
use crate::rows;
use crate::text::Font;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The chart section of a resolved style, as the report payload carries it (T-STYLE-CHART,
/// STY-18, STY-39, STY-40). Colours are CSS colour strings the engine parses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChartTheme {
    /// `bg-chart`: the chart surface.
    pub surface: String,
    /// `bg`: the page around the chart.
    pub page: String,
    /// `chart-ink`.
    pub ink: String,
    pub ink_soft: String,
    pub ink_faint: String,
    pub rule: String,
    pub rule_soft: String,
    pub accent: String,
    pub palette: Vec<String>,
    /// Three-stop sequential ramp for heatmaps.
    pub ramp: Vec<String>,
    /// Three-stop ramp for negative values; empty to turn the hue 150° (FEA-CH-10, STY-56).
    pub negative_ramp: Vec<String>,
    /// Data face (STY-39).
    pub data_font: String,
    /// Tabular-figure variant of the data face, when the style has one (OPN-62).
    pub figures_font: Option<String>,
    pub ui_font: String,
    /// Chart text size in px.
    pub size: f64,
    pub bar_radius: f64,
    pub decals: bool,
    /// Heatmap cells below zero carry a diagonal hatch, for styles whose arms share one hue.
    pub sign_hatch: bool,
    pub curve: String,
    pub curvature: f64,
    pub motion: bool,
    /// The style's entrance timing, which the engine's defaults give way to (INT-12).
    pub motion_profile: crate::motion::Profile,
    /// Legend position: `top` or `bottom` (STY-01, FEA-CH-33).
    pub legend: String,
    /// `down`: what needs the reader's eye, such as a flow chart's failure path.
    pub down: String,
}

impl Default for ChartTheme {
    fn default() -> Self {
        // Air, the light builtin, so engine tests read like a printed page.
        Self {
            surface: "oklch(1 0 0)".into(),
            page: "oklch(1 0 0)".into(),
            ink: "oklch(0.2 0.01 260)".into(),
            ink_soft: "oklch(0.45 0.012 260)".into(),
            ink_faint: "oklch(0.62 0.01 260)".into(),
            rule: "oklch(0.85 0.005 260)".into(),
            rule_soft: "oklch(0.92 0.004 260)".into(),
            accent: "oklch(0.5464 0.2508 263.51)".into(),
            palette: [
                "oklch(0.546 0.251 263.5)",
                "oklch(0.64 0.1 188)",
                "oklch(0.56 0.2 335)",
                "oklch(0.56 0.2 33)",
                "oklch(0.66 0.14 72)",
                "oklch(0.58 0.12 287)",
                "oklch(0.56 0.18 142)",
                "oklch(0.66 0.24 3)",
            ]
            .map(String::from)
            .to_vec(),
            ramp: ["oklch(0.97 0.015 262)", "oklch(0.72 0.12 262)", "oklch(0.46 0.21 264)"]
                .map(String::from)
                .to_vec(),
            negative_ramp: Vec::new(),
            data_font: "Geist Mono, ui-monospace, monospace".into(),
            figures_font: None,
            ui_font: "Geist, system-ui, sans-serif".into(),
            size: 12.0,
            bar_radius: 3.0,
            decals: false,
            sign_hatch: false,
            curve: "monotone".into(),
            curvature: 1.0,
            motion: true,
            motion_profile: crate::motion::Profile::default(),
            legend: "top".into(),
            down: "oklch(0.55 0.17 27)".into(),
        }
    }
}

/// Render-time inputs that are not part of the block (TYP-71).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// Width the chart occupies, in CSS pixels.
    pub width: f64,
    /// Report language (TYP-08).
    pub lang: String,
    /// Report-level entity colours: category value to 1-based palette slot (FEA-CH-63).
    pub colors: Vec<(String, usize)>,
    /// Series or category keys the reader has hidden (FEA-CH-33).
    pub hidden: Vec<String>,
    /// Zoom window along x in data units (FEA-CH-49); `None` shows all data.
    pub zoom: Option<[f64; 2]>,
    /// The room is wider than the chart: a key the chart draws itself stands beside its plot,
    /// not under it. The expanded view asks for it in a short, wide window, where a chart
    /// narrowed to fit the height leaves width on its sides (S27-875).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub aside: bool,
    /// Extents the scales cover beyond the rows drawn: a playback fits every frame (FEA-BR-31).
    pub fit: Option<Fit>,
    /// Structures a layout from the exact rows derived; with these a layout from grid rows
    /// draws the same marks (TYP-36). The caller selects the records of the current author
    /// control state; legend toggles and zoom retain that structure.
    pub derived: Vec<crate::derived::Derived>,
    /// The report's column grids, shared by layout records that carry data-scale positions.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub grids: std::collections::BTreeMap<String, crate::payload::Quant>,
}

/// Data extents a chart's scales cover. A layout records the extents it used in
/// [`Geometry::fit`](crate::geometry::Geometry); laid out over every frame of a playback, they
/// keep the value axes, size and colour scales and categories still while frames change
/// (FEA-BR-31). `None` and empty fields follow the rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Fit {
    pub x: Option<[f64; 2]>,
    pub y: Option<[f64; 2]>,
    /// Largest bubble size.
    pub size: Option<f64>,
    /// Smallest positive bubble size, for the size key's low end.
    pub size_min: Option<f64>,
    /// A heatmap's colour range.
    pub color: Option<[f64; 2]>,
    /// Category names in axis order.
    pub categories: Vec<String>,
    /// The points a playback names in every frame, by their label: those its opening frame
    /// names by default, so a name travels with its point instead of jumping to whichever point
    /// is extreme in each frame (FEA-CH-28, FEA-BR-31).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
}

impl Fit {
    /// `extent` grown to cover `other`.
    pub(crate) fn widen(extent: [f64; 2], other: Option<[f64; 2]>) -> [f64; 2] {
        match other {
            Some([a, b]) if a.is_finite() && b.is_finite() => [extent[0].min(a), extent[1].max(b)],
            _ => extent,
        }
    }
}

/// Fixed chart chrome (STY-40).
pub(crate) const LINE_WIDTH: f64 = 2.5;
pub(crate) const GRID_WIDTH: f64 = 1.0;
pub(crate) const BASELINE_WIDTH: f64 = 1.5;
pub(crate) const AREA_OPACITY: f64 = 0.15;
pub(crate) const BAND_OPACITY: f64 = 0.28;
pub(crate) const POINT_RADIUS: f64 = 4.0;
pub(crate) const POINT_OPACITY: f64 = 0.72;
pub(crate) const BUBBLE_MAX: f64 = 16.0;
/// Least plot height at any width (FEA-CH-65).
pub(crate) const MIN_PLOT: f64 = 120.0;
/// Gap between axis labels and the plot.
pub(crate) const LABEL_GAP: f64 = 8.0;

/// The style's resolved colours and faces.
#[derive(Debug, Clone)]
pub(crate) struct Style {
    pub surface: Oklch,
    pub ink: Oklch,
    pub ink_soft: Oklch,
    pub ink_faint: Oklch,
    pub rule_soft: Oklch,
    pub palette: Vec<Oklch>,
    pub ramp: [Oklch; 3],
    pub negative_ramp: Option<[Oklch; 3]>,
    pub font: Font,
    pub figures: Font,
    /// The interface face, for prose a monospaced chart face would set badly.
    pub ui: Font,
    pub alert: Oklch,
    pub size: f64,
    pub bar_radius: f64,
    pub decals: bool,
    pub sign_hatch: bool,
    pub curve: Curve,
    pub curvature: f64,
    pub motion: bool,
    pub motion_profile: crate::motion::Profile,
    pub legend_bottom: bool,
}

fn parse_color(css: &str, fallback: Oklch) -> Oklch {
    color::parse(css).unwrap_or(fallback)
}

impl Style {
    pub fn from_theme(theme: &ChartTheme) -> Style {
        let black = Oklch::new(0.2, 0.0, 0.0);
        let white = Oklch::new(1.0, 0.0, 0.0);
        let surface = parse_color(&theme.surface, white);
        let three = |stops: &[String]| -> Option<[Oklch; 3]> {
            let parsed: Vec<Oklch> = stops.iter().filter_map(|s| color::parse(s)).collect();
            (parsed.len() == 3).then(|| [parsed[0], parsed[1], parsed[2]])
        };
        let mut palette: Vec<Oklch> = theme.palette.iter().filter_map(|c| color::parse(c)).collect();
        if palette.is_empty() {
            palette.push(parse_color(&theme.accent, black));
        }
        Style {
            surface,
            ink: parse_color(&theme.ink, black),
            ink_soft: parse_color(&theme.ink_soft, black),
            ink_faint: parse_color(&theme.ink_faint, black),
            rule_soft: parse_color(&theme.rule_soft, black),
            palette,
            ramp: three(&theme.ramp).unwrap_or([
                Oklch::new(0.97, 0.015, 262.0),
                Oklch::new(0.72, 0.12, 262.0),
                Oklch::new(0.46, 0.21, 264.0),
            ]),
            negative_ramp: three(&theme.negative_ramp),
            font: Font::new(&theme.data_font),
            figures: Font::new(theme.figures_font.as_deref().unwrap_or(&theme.data_font)),
            ui: Font::new(&theme.ui_font),
            alert: parse_color(&theme.down, Oklch::new(0.55, 0.17, 27.0)),
            size: if theme.size > 0.0 { theme.size } else { 12.0 },
            bar_radius: theme.bar_radius.max(0.0),
            decals: theme.decals,
            sign_hatch: theme.sign_hatch,
            curve: Curve::parse(&theme.curve).unwrap_or_default(),
            curvature: theme.curvature.clamp(0.0, 1.0),
            motion: theme.motion,
            motion_profile: theme.motion_profile.clone(),
            legend_bottom: theme.legend == "bottom",
        }
    }
    pub fn rgba(c: Oklch) -> Rgba {
        Rgba::from_oklch(c, 1.0)
    }
}

impl Ctx<'_> {
    /// The locale of calendar labels on the x axis: its own `locale`, else the report's (TYP-16).
    pub(crate) fn calendar_locale(&self) -> Locale {
        self.block.x.locale.unwrap_or(self.locale)
    }
}

/// Everything a painter needs while it lays out one chart.
pub(crate) struct Ctx<'a> {
    pub block: &'a ChartBlock,
    pub rows: &'a [Value],
    pub style: Style,
    pub opts: &'a Options,
    pub locale: Locale,
    /// What the layouts of this data state share (see [`Prepared`]).
    pub prepared: &'a Prepared,
    /// Outgoing per-panel buffers, consumed by an eligible line painter during this call.
    pub observations: Vec<Vec<Observation>>,
    pub series: Vec<Series>,
    pub checks: Vec<Check>,
    pub dropped: Vec<Dropped>,
    pub note: Vec<String>,
    /// One-sentence summary for the figure's accessible name (FEA-CH-55).
    pub summary: String,
    /// The extents the painter used (FEA-BR-31).
    pub fit: Fit,
    /// Line and area: for each series of the panel just painted, which of its observations hold
    /// the series' last, first, lowest and highest value over all its data, or `None` outside the
    /// zoom window; the automatic value labels name these at every zoom (FEA-CH-28, FEA-CH-49).
    pub value_kinds: Vec<[Option<usize>; 4]>,
    /// What the painters derived, recorded for the geometry (TYP-36).
    pub derived: std::cell::RefCell<Vec<crate::derived::Derived>>,
}

impl<'a> Ctx<'a> {
    /// A structure that jumps with the data, computed once from the exact rows and handed back
    /// as the geometry's `derived` list (TYP-36). A layout given the record in its options
    /// returns that instead of computing: grid rows then draw the marks the exact rows drew.
    /// The caller selects the author control state; reader interactions do not invalidate it.
    /// `T` holds data-space numbers and names only, never pixels, so every width can use it; the
    /// record keeps six significant digits and the caller gets the rounded copy either way.
    pub fn derive<T>(&self, panel: usize, key: &str, compute: impl FnOnce() -> T) -> T
    where
        T: Serialize + serde::de::DeserializeOwned,
    {
        self.derive_at(panel, key, true, compute)
    }

    /// A total, share or its label placement that changes with the visible series. The runtime
    /// computes a missing record from shown values and hands it to the geometry layout (TYP-36).
    pub fn derive_visible<T>(&self, panel: usize, key: &str, compute: impl FnOnce() -> T) -> T
    where
        T: Serialize + serde::de::DeserializeOwned,
    {
        if self.opts.hidden.is_empty() {
            self.derive(panel, key, compute)
        } else {
            self.derive(panel, &format!("{key}.hidden.{:?}", self.opts.hidden), compute)
        }
    }

    /// Replay a record whose data values are already encoded as grid indices or shown values.
    /// Keep its grid metadata intact; applying layout-coordinate rounding would change its scale.
    pub fn derive_projected<T>(&self, panel: usize, key: &str, compute: impl FnOnce() -> T) -> T
    where
        T: Serialize + serde::de::DeserializeOwned,
    {
        self.derive_at(panel, key, false, compute)
    }

    /// A projected part-to-whole record keyed by the currently visible series.
    pub fn derive_visible_projected<T>(&self, panel: usize, key: &str, compute: impl FnOnce() -> T) -> T
    where
        T: Serialize + serde::de::DeserializeOwned,
    {
        if self.opts.hidden.is_empty() {
            self.derive_projected(panel, key, compute)
        } else {
            self.derive_projected(panel, &format!("{key}.hidden.{:?}", self.opts.hidden), compute)
        }
    }

    fn derive_at<T>(&self, panel: usize, key: &str, round_layout: bool, compute: impl FnOnce() -> T) -> T
    where
        T: Serialize + serde::de::DeserializeOwned,
    {
        let given = self.opts.derived.iter().find(|d| d.panel == panel && d.key == key);
        if let Some(d) = given
            && let Ok(t) = T::deserialize(&d.value)
        {
            self.derived.borrow_mut().push(d.clone());
            return t;
        }
        let computed = compute();
        let Ok(mut value) = serde_json::to_value(&computed) else {
            return computed;
        };
        let result = if round_layout {
            crate::derived::round_value(&mut value);
            T::deserialize(&value).unwrap_or(computed)
        } else {
            computed
        };
        self.derived.borrow_mut().push(crate::derived::Derived {
            panel,
            key: key.to_owned(),
            value,
        });
        result
    }

    /// The supplied column grid, or the same shared-scale grid constructed for a native caller.
    pub fn grid_for(&self, field: &str) -> Option<crate::payload::Quant> {
        if let Some(grid) = self.opts.grids.get(field) {
            return Some(grid.clone());
        }
        let scale = self.block.scale_of(field);
        let fields: Vec<_> = self
            .block
            .referenced_fields()
            .into_iter()
            .filter(|f| f == field || (scale.is_some() && self.block.scale_of(f) == scale))
            .map(|f| {
                let axis = if self.block.encoding.x.as_deref() == Some(&f) {
                    &self.block.x
                } else if self.block.encoding.color.as_deref() == Some(&f) {
                    &self.block.color
                } else {
                    &self.block.y
                };
                let format = self.block.readout_format(axis, self.locale);
                (f, format)
            })
            .collect();
        let values: Vec<(f64, f64)> = self
            .rows
            .iter()
            .flat_map(|r| {
                fields
                    .iter()
                    .filter_map(move |(f, format)| match crate::rows::num(r, f) {
                        crate::rows::Num::Value(n) => Some((n, crate::numbers::shown(n, format))),
                        _ => None,
                    })
            })
            .collect();
        crate::payload::Quant::for_values(values.iter().map(|v| v.0), values.iter().map(|v| v.1))
    }

    /// A value extent decided by the exact rows (see [`Ctx::derive`]). An empty extent, which is
    /// infinite and cannot be recorded, comes back as it started.
    pub fn derive_extent(&self, panel: usize, key: &str, compute: impl FnOnce() -> [f64; 2]) -> [f64; 2] {
        let found: Option<[f64; 2]> = self.derive(panel, key, || {
            let e = crate::derived::coarse(compute());
            (e[0].is_finite() && e[1].is_finite()).then_some(e)
        });
        found.unwrap_or([f64::INFINITY, f64::NEG_INFINITY])
    }

    /// The extents the reader's playback asks the scales to cover.
    pub fn wanted(&self) -> Fit {
        self.opts.fit.clone().unwrap_or_default()
    }

    pub fn size(&self) -> f64 {
        self.style.size
    }
    pub fn line_height(&self) -> f64 {
        self.style.font.line_height(self.size())
    }
    pub fn text_width(&self, text: &str) -> f64 {
        self.style.font.width(text, self.size())
    }

    /// One line of chart text with its laid-out box.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&self, text: &str, x: f64, y: f64, anchor: Anchor, face: Face, role: Role, color: Oklch) -> Text {
        let font = if face == Face::Figures {
            &self.style.figures
        } else {
            &self.style.font
        };
        let size = self.size();
        // Axis furniture is secondary text in the style's regular weight (STY-39); only data
        // labels, panel titles and notes take the primary ink.
        let color = match role {
            Role::Tick | Role::AxisLabel | Role::AxisTitle => self.style.ink_soft,
            _ => color,
        };
        Text {
            text: text.to_owned(),
            x,
            y,
            anchor,
            face,
            size,
            weight: 400,
            color: Style::rgba(color),
            rotate: 0.0,
            bounds: font.bounds(text, size, x, y, anchor),
            role,
            series: None,
            mark: None,
            near: false,
        }
    }

    /// Text whose box is vertically centred on `cy`.
    #[allow(clippy::too_many_arguments)]
    pub fn text_mid(&self, text: &str, x: f64, cy: f64, anchor: Anchor, face: Face, role: Role, color: Oklch) -> Text {
        let (ascent, descent) = self.style.font.vertical();
        let baseline = cy + (ascent - descent) * self.size() / 2.0;
        self.text(text, x, baseline, anchor, face, role, color)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn check(
        &mut self,
        kind: &str,
        level: Level,
        message: impl Into<String>,
        fix: Option<&str>,
        path: Option<String>,
        region: Option<Rect>,
        details: Value,
    ) {
        self.checks.push(Check {
            kind: kind.into(),
            level,
            message: message.into(),
            fix: fix.map(str::to_owned),
            path,
            region,
            facet: None,
            details,
        });
    }

    pub fn drop_label(&mut self, text: &str, reason: DropReason, kind: &str, at: Option<Rect>) {
        self.dropped.push(Dropped {
            text: text.to_owned(),
            reason,
            kind: Some(kind.to_owned()),
            would_be_at: at,
            observation: None,
            series: None,
        });
    }

    pub fn series_color(&self, index: usize) -> Oklch {
        let slot = self.series.get(index).map_or(0, |s| s.slot);
        self.style.palette[slot % self.style.palette.len()]
    }

    /// `color` as text on the chart surface: its lightness moves away from the surface until the
    /// text reaches 4.5:1, so series-coloured names stay legible on every style (STY-38).
    pub fn readable(&self, color: Oklch) -> Oklch {
        let surface = self.style.surface;
        let lighter = surface.l < 0.5;
        let mut c = color;
        for _ in 0..50 {
            if color::contrast(c, surface) >= 4.5 {
                break;
            }
            c.l = (c.l + if lighter { 0.02 } else { -0.02 }).clamp(0.0, 1.0);
        }
        c
    }

    /// Identify series or categories, then switch decals on where the palette fails its rule
    /// on this chart's surface (FEA-CH-57, FEA-CH-62).
    pub fn set_series(&mut self, keys: &[String]) {
        self.series = identify(keys, &self.style, self.opts);
        checks::palette(self);
    }

    pub fn is_hidden(&self, index: usize) -> bool {
        self.series.get(index).is_some_and(|s| s.hidden)
    }

    /// Numeric text too small to represent: a source error at its row and field that names the
    /// value, in every channel (coordinates, measures, colour values, interval bounds and
    /// estimates, bubble sizes). The value is never drawn as zero (FEA-CH-70). Each channel
    /// otherwise treats the row as it treats text that is not a number: coordinates, histogram,
    /// box-plot and interval values fail their panel, while a pie value, heatmap colour or bubble
    /// size leaves its row out.
    pub fn underflow(&mut self, row: usize, field: &str, text: &str) {
        self.check(
            "source-error",
            Level::Error,
            format!("Row {} has {field} '{text}', too small to represent", row + 1),
            Some("Scale the values or use zero if zero is meant."),
            Some(Self::row_path(row, field)),
            None,
            json!({"row": row, "value": text}),
        );
    }

    /// Path into a resolved row: `rows[12].sales` (TYP-53).
    pub fn row_path(row: usize, field: &str) -> String {
        format!("rows[{row}].{field}")
    }
}

/// Series or categories in first-seen order, with palette slots that the report's entity map
/// fixes and other values fill in source order (FEA-CH-09, FEA-CH-61, FEA-CH-63).
pub(crate) fn identify(keys: &[String], style: &Style, opts: &Options) -> Vec<Series> {
    let mut taken: Vec<usize> = Vec::new();
    let mut slots: Vec<Option<usize>> = keys
        .iter()
        .map(|k| {
            opts.colors.iter().find(|(name, _)| name == k).map(|(_, slot)| {
                let s = slot.saturating_sub(1);
                taken.push(s);
                s
            })
        })
        .collect();
    let mut next = 0;
    for slot in slots.iter_mut().filter(|s| s.is_none()) {
        while taken.contains(&next) {
            next += 1;
        }
        *slot = Some(next);
        taken.push(next);
    }
    let decals = style.decals;
    keys.iter()
        .zip(slots)
        .map(|(key, slot)| {
            let slot = slot.unwrap_or(0);
            let c = style.palette[slot % style.palette.len()];
            Series {
                key: key.clone(),
                name: key.clone(),
                slot,
                color: Style::rgba(c),
                hatch: decals.then(|| Hatch::for_slot(slot)),
                shape: decals.then(|| Shape::for_slot(slot)),
                dash: if decals { dash_for(slot) } else { Vec::new() },
                hidden: opts.hidden.iter().any(|h| h == key),
            }
        })
        .collect()
}

/// Distinct dash patterns per slot; slot 0 stays solid.
pub(crate) fn dash_for(slot: usize) -> Vec<f64> {
    match slot % 6 {
        0 => vec![],
        1 => vec![6.0, 3.0],
        2 => vec![2.0, 2.5],
        3 => vec![8.0, 3.0, 2.0, 3.0],
        4 => vec![12.0, 4.0],
        _ => vec![3.0, 5.0],
    }
}

/// Switch decals on for every series (FEA-CH-57, FEA-CH-62).
pub(crate) fn decals_on(series: &mut [Series]) {
    for s in series {
        s.hatch = Some(Hatch::for_slot(s.slot));
        s.shape = Some(Shape::for_slot(s.slot));
        s.dash = dash_for(s.slot);
    }
}

/// What the layouts of one data state share while only the width, the ratio or the zoom changes:
/// the rows read and each readout formatted once, so a zoom or resize frame costs the geometry of
/// what it draws (PRF-19).
///
/// A preparation belongs to one data state: the block apart from `style.ratio`, the rows, the
/// theme, and the options apart from `width`, `zoom` and the records derived for a zoom. So a
/// filter, a playback frame, a hidden series, the language and a style each need a new one. Layout
/// fills it on first use and reads it after that; a new one gives what [`layout`] gives.
#[derive(Default)]
pub struct Prepared {
    facets: std::cell::OnceCell<Vec<(Option<String>, Vec<usize>)>>,
    cartesian: std::cell::OnceCell<cartesian::Data>,
}

impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prepared")
            .field("filled", &self.facets.get().is_some())
            .finish_non_exhaustive()
    }
}

/// Lay out one chart.
pub fn layout(block: &ChartBlock, rows: &[Value], theme: &ChartTheme, opts: &Options) -> Geometry {
    layout_prepared(block, rows, theme, opts, &Prepared::default())
}

/// [`layout`] reading the rows and readouts from `prepared`, which must belong to this data state.
pub fn layout_prepared(
    block: &ChartBlock,
    rows: &[Value],
    theme: &ChartTheme,
    opts: &Options,
    prepared: &Prepared,
) -> Geometry {
    layout_prepared_reusing(block, rows, theme, opts, prepared, Vec::new())
}

/// Lay out a chart by refilling outgoing line observation and dot buffers when eligible.
///
/// `observations` owns one vector per facet in panel order. Automatic, non-stacked lines
/// reuse its slots; other charts discard it. Every slot receives fresh fields and readouts,
/// and unused slots are dropped before returning. The result matches [`layout_prepared`].
/// `prepared` must belong to the current data state, as for [`layout_prepared`]; the outgoing
/// buffers may come from a different layout and supply storage only.
pub fn layout_prepared_reusing(
    block: &ChartBlock,
    rows: &[Value],
    theme: &ChartTheme,
    opts: &Options,
    prepared: &Prepared,
    observations: Vec<Vec<Observation>>,
) -> Geometry {
    let mut style = Style::from_theme(theme);
    // A chart's own palette, by builtin name or as colours, wins over the style's (STY-15, STY-53).
    if let Some(palette) = &block.style.palette {
        let colors: Vec<Oklch> = match palette {
            PaletteRef::Named(name) => crate::rmd_render::palette(name)
                .iter()
                .filter_map(|c| color::parse(c))
                .collect(),
            PaletteRef::Colors(list) => list.iter().filter_map(|c| color::parse(c)).collect(),
        };
        if !colors.is_empty() {
            style.palette = colors;
        }
    }
    let locale = Locale::from_tag(if opts.lang.is_empty() { "en" } else { &opts.lang });
    let width = if opts.width.is_finite() && opts.width > 0.0 {
        opts.width.clamp(240.0, 4096.0)
    } else {
        640.0
    };
    let normalized;
    let opts = if width == opts.width {
        opts
    } else {
        normalized = Options { width, ..opts.clone() };
        &normalized
    };
    let mut ctx = Ctx {
        block,
        rows,
        style,
        opts,
        locale,
        prepared,
        observations,
        series: Vec::new(),
        checks: Vec::new(),
        dropped: Vec::new(),
        note: Vec::new(),
        summary: String::new(),
        fit: Fit::default(),
        value_kinds: Vec::new(),
        derived: std::cell::RefCell::new(Vec::new()),
    };
    let facets = prepared.facets.get_or_init(|| facet_groups(block, rows));
    // Rows without a facet value sit in no panel; they are disclosed, never dropped silently
    // (FEA-CH-18).
    if let Some(field) = &block.encoding.facet {
        let placed: usize = facets.iter().map(|(_, m)| m.len()).sum();
        let missing = rows.len() - placed;
        if missing > 0 && !facets.is_empty() {
            ctx.check(
                "excluded-observations",
                Level::Warning,
                format!(
                    "{} without a {field} value {} left out",
                    count(missing, "row", "rows"),
                    verb(missing, "is", "are")
                ),
                Some("Give every row a value for the facet field."),
                Some("chart.encoding.facet".into()),
                None,
                json!({"count": missing}),
            );
            ctx.note.push(format!(
                "{} without a panel {} not shown",
                count(missing, "row", "rows"),
                verb(missing, "is", "are")
            ));
        }
    }
    let empty = if rows.is_empty() {
        Some("Nothing to draw · The data has no rows".to_owned())
    } else if facets.is_empty() {
        let field = block.encoding.facet.as_deref().unwrap_or_default();
        Some(format!("Nothing to draw · No row has a {field} value"))
    } else {
        None
    };
    let mut out = if let Some(reason) = empty {
        // A chart never disappears from the report (FEA-CH-18).
        ctx.check(
            "empty-data",
            Level::Error,
            reason.clone(),
            Some("Give the chart rows to draw."),
            Some("chart.data".into()),
            None,
            Value::Null,
        );
        let height = (width / block.kind.default_ratio(false)).max(MIN_PLOT);
        let panel = failed_panel(&ctx, Rect::new(0.0, 0.0, width, height), &reason);
        Painted {
            height,
            panels: vec![panel],
            figure_items: Vec::new(),
            legend: None,
            failure: Some(reason),
        }
    } else {
        (types::def(block.kind).paint)(&mut ctx, facets)
    };
    // A chart whose every panel failed has failed (FEA-CH-18).
    if out.failure.is_none() && !out.panels.is_empty() && out.panels.iter().all(|p| p.failure.is_some()) {
        out.failure = out.panels[0].failure.clone();
    }
    if let Some(field) = &block.encoding.facet {
        for (panel, (name, _)) in out.panels.iter_mut().zip(facets) {
            panel.facet = name.clone();
            let _ = field;
        }
    }
    // Row-level findings name the panel their row sits in.
    if let Some(field) = &block.encoding.facet {
        for check in ctx.checks.iter_mut().filter(|c| c.facet.is_none()) {
            let row = check
                .path
                .as_deref()
                .and_then(|p| p.strip_prefix("rows["))
                .and_then(|p| p.split(']').next());
            if let Some(row) = row.and_then(|r| r.parse::<usize>().ok()).and_then(|r| rows.get(r)) {
                check.facet = crate::rows::text(row, field);
            }
        }
    }
    finish::finish(
        &mut out,
        width,
        Style::rgba(ctx.style.surface),
        Style::rgba(ctx.style.ink),
    );
    checks::run(&mut ctx, &mut out);
    let entrance = ctx.style.motion.then(|| entrance(block.kind, &out));
    let summary = if ctx.summary.is_empty() {
        format!("{} chart", block.kind.name())
    } else {
        ctx.summary.clone()
    };
    let mut geometry = Geometry {
        kind: block.kind.name().to_owned(),
        width,
        height: out.height,
        background: Style::rgba(ctx.style.surface),
        series: ctx.series,
        legend: out.legend,
        panels: out.panels,
        figure_items: out.figure_items,
        note: (!ctx.note.is_empty()).then(|| ctx.note.join(" · ")),
        summary,
        checks: ctx.checks,
        dropped: ctx.dropped,
        entrance,
        timing: ctx.style.motion_profile.clone(),
        failure: out.failure,
        fit: ctx.fit,
        derived: ctx.derived.into_inner(),
    };
    geometry.attribute_marks();
    geometry
}

/// What a painter returns.
pub(crate) struct Painted {
    pub height: f64,
    pub panels: Vec<Panel>,
    pub figure_items: Vec<crate::geometry::Item>,
    pub legend: Option<crate::geometry::Legend>,
    pub failure: Option<String>,
}

/// Facet panels in source order, each with its rows; one unnamed panel without `facet`.
pub(crate) fn facet_groups(block: &ChartBlock, data: &[Value]) -> Vec<(Option<String>, Vec<usize>)> {
    match &block.encoding.facet {
        None => vec![(None, (0..data.len()).collect())],
        Some(field) => {
            let names = rows::distinct(data, field);
            names
                .into_iter()
                .map(|name| {
                    let members = (0..data.len())
                        .filter(|&i| rows::text(&data[i], field).as_deref() == Some(&name))
                        .collect();
                    (Some(name), members)
                })
                .collect()
        }
    }
}

/// Facet grid: two columns when the figure is at least 600 px wide (FEA-CH-35).
pub(crate) fn facet_columns(count: usize, width: f64) -> usize {
    if count > 1 && width >= 600.0 { 2 } else { 1 }
}

fn entrance(kind: ChartType, out: &Painted) -> Entrance {
    let marks = out
        .panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| i.role() == Role::Mark)
        .count();
    let base = types::def(kind).entrance;
    if base != Entrance::Sweep && marks >= 2000 {
        Entrance::Fade
    } else {
        base
    }
}

/// A chart or panel that cannot draw: the reason centred in its plot (FEA-CH-18).
/// `1 category`, `3 categories`.
pub(crate) fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The verb that agrees with a count: `is` for 1 and `are` for 3.
pub(crate) fn verb<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// A facet panel's title above its plot.
pub(crate) fn panel_title(ctx: &Ctx, panel: &mut Panel, name: &Option<String>) {
    if let Some(name) = name {
        let t = ctx.text(
            name,
            panel.frame.x,
            panel.frame.y + ctx.style.font.baseline_offset(ctx.size()),
            Anchor::Start,
            Face::Data,
            Role::PanelTitle,
            ctx.style.ink,
        );
        panel
            .items
            .push(crate::geometry::Item::Text(crate::geometry::Text { weight: 600, ..t }));
    }
}

/// A facet panel that cannot draw: its title, and its reason centred below it, while the other
/// panels draw (FEA-CH-18, FEA-CH-35).
pub(crate) fn failed_facet(ctx: &Ctx, frame: Rect, title_room: f64, name: &Option<String>, reason: &str) -> Panel {
    let body = Rect::new(frame.x, frame.y + title_room, frame.w, (frame.h - title_room).max(1.0));
    let mut panel = failed_panel(ctx, body, reason);
    panel.frame = frame;
    panel_title(ctx, &mut panel, name);
    panel
}

/// Each row's facet panel, `None` for rows that sit in no panel.
pub(crate) fn facet_index(facets: &[(Option<String>, Vec<usize>)], rows: usize) -> Vec<Option<usize>> {
    let mut out = vec![None; rows];
    for (i, (_, members)) in facets.iter().enumerate() {
        for &r in members {
            out[r] = Some(i);
        }
    }
    out
}

pub(crate) fn failed_panel(ctx: &Ctx, frame: Rect, reason: &str) -> Panel {
    let text = ctx.text_mid(
        reason,
        frame.x + frame.w / 2.0,
        frame.y + frame.h / 2.0,
        Anchor::Middle,
        Face::Data,
        Role::Notice,
        ctx.style.ink_soft,
    );
    Panel {
        facet: None,
        frame,
        plot: frame,
        items: vec![crate::geometry::Item::Text(text)],
        observations: Vec::new(),
        targeting: crate::geometry::Targeting::Contain,
        x: None,
        failure: Some(reason.to_owned()),
    }
}

/// The figure's height for a panel grid whose panels have `panel_height` each.
pub(crate) fn grid_height(count: usize, columns: usize, panel_height: f64, gap: f64) -> f64 {
    let rows = count.div_ceil(columns.max(1));
    rows as f64 * panel_height + rows.saturating_sub(1) as f64 * gap
}

/// Frames of a facet grid of `count` panels.
pub(crate) fn grid_frames(count: usize, width: f64, top: f64, panel_height: f64) -> Vec<Rect> {
    let columns = facet_columns(count, width);
    let gap_x = 24.0;
    let gap_y = 20.0;
    let panel_width = (width - gap_x * (columns as f64 - 1.0)) / columns as f64;
    (0..count)
        .map(|i| {
            let (r, c) = (i / columns, i % columns);
            Rect::new(
                c as f64 * (panel_width + gap_x),
                top + r as f64 * (panel_height + gap_y),
                panel_width,
                panel_height,
            )
        })
        .collect()
}

pub(crate) const FACET_GAP_Y: f64 = 20.0;

/// The rows a flow chart written as `nodes` and `edges` shows in its Table view and CSV: one per
/// card, then one per edge (FEA-CH-76).
pub fn flow_table_rows(block: &Value) -> Option<Vec<Value>> {
    flow::table_rows(block)
}
