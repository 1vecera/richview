//! Line, area, bar and scatter charts: one painter with a numeric, calendar or category x
//! (FEA-CH-02 to FEA-CH-07, FEA-CH-15, FEA-CH-17, FEA-CH-19, FEA-CH-26 to FEA-CH-28,
//! FEA-CH-38, FEA-CH-72).

use super::place;
use super::{
    AREA_OPACITY, BAND_OPACITY, BASELINE_WIDTH, BUBBLE_MAX, Ctx, Fit, GRID_WIDTH, LABEL_GAP, LINE_WIDTH, MIN_PLOT,
    POINT_OPACITY, POINT_RADIUS, Painted, Style,
};
use crate::axis::{self, CalendarAxis, CategoryLayout, Ordered, ValueAxis, ValueSpec, Zero};
use crate::block::{ChartType, Orientation};
use crate::curve;
use crate::diag::Level;
use crate::geometry::{
    Anchor, Cap, Cmd, Dot, DropReason, Face, Fill, Item, Legend, Observation, Panel, Path, Readout, ReadoutLine, Rect,
    Role, Shape, Stroke, Swatch, Targeting, XScale,
};
use crate::numbers::{self, Format, Kind};
use crate::rows::{self, Num, XKind};
use crate::time::{self, Precision};
use serde_json::{Value, json};
use std::rc::Rc;

/// One source row read for this chart.
#[derive(Debug, Clone)]
struct Obs {
    row: usize,
    series: usize,
    /// Position along x in data units: milliseconds, a number, or a category index.
    x: f64,
    y: Option<f64>,
    size: Option<f64>,
}

/// The x scale shared by every panel (or computed per panel when scales are free).
#[derive(Debug, Clone)]
enum XModel {
    Calendar {
        precision: Precision,
        spacing: i64,
        extent: [f64; 2],
    },
    Numeric {
        extent: [f64; 2],
        years: bool,
    },
    Category {
        names: Vec<String>,
        reader: Vec<String>,
        ordered: Option<Ordered>,
    },
}

struct Parsed {
    obs: Vec<Obs>,
    x: XModel,
    /// Series count drawn as distinct groups (bars side by side, colours).
    groups: usize,
    /// Bars coloured by their own category (FEA-CH-72).
    self_colour: bool,
    /// Per facet panel: why it cannot draw. A bad panel fails alone (FEA-CH-18, FEA-CH-35).
    failures: Vec<Option<String>>,
    /// Each row's facet panel.
    panel_of: Vec<Option<usize>>,
}

impl Parsed {
    /// Observations in panels that draw: what the summary may describe.
    fn drawn(&self) -> impl Iterator<Item = &Obs> {
        self.obs
            .iter()
            .filter(|o| self.panel_of[o.row].is_some_and(|f| self.failures[f].is_none()))
    }
}

/// A cartesian chart's rows as one data state reads them (see [`super::Prepared`]): its series,
/// how x is scaled, and the rest once the series are known.
pub(crate) struct Data {
    /// Series identity across every panel, in first-seen order.
    keys: Vec<String>,
    x_kind: XKind,
    read: std::cell::OnceCell<Read>,
}

/// The observations, with what reading them reported, and each panel's in the order it draws them.
struct Read {
    parsed: Result<Parsed, String>,
    /// Rows in drawable panels, independent of viewport width and zoom.
    drawn: Vec<usize>,
    /// Valid positions across the data state, counted once before zoom or legend hiding.
    points: usize,
    /// The checks and notes reading the rows made, reported by every layout of the state.
    checks: Vec<crate::geometry::Check>,
    notes: Vec<String>,
    /// Line and area: each observation's value at the author's precision, once a layout needs it
    /// (NaN until then).
    shown: Vec<std::cell::Cell<f64>>,
    panels: Vec<std::cell::OnceCell<Members>>,
    /// Value extents by panel and name, as [`value_extent`] finds them.
    extents: std::cell::RefCell<Vec<(usize, &'static str, [f64; 2])>>,
    summary: std::cell::OnceCell<String>,
}

/// One facet panel's observations in the order its painter reads them. Each readout is made when
/// a layout first shows its mark, so a layout on its own formats no more than it shows.
struct Members {
    /// Indices into [`Parsed::obs`], in row order.
    obs: Vec<usize>,
    /// This panel's source x extent; viewport fitting applies its padding per layout.
    x_extent: [f64; 2],
    /// A scatter whose source order already follows x can bisect a zoom without reordering.
    x_ordered: bool,
    /// Line and area: each series' observations in x order.
    series: Vec<Vec<usize>>,
    /// Line and area: one readout per distinct x, in x order (FEA-CH-41).
    at_x: Vec<AtX>,
    /// Line and area: for each series, the distinct x of its last, first, lowest and highest
    /// value over all its data (FEA-CH-28).
    kinds: Vec<[Option<usize>; 4]>,
    /// Scatter: each observation's readout, parallel to `obs`.
    readouts: Vec<std::cell::OnceCell<Rc<Readout>>>,
    /// Reusable scatter hash-table storage, emptied before and after each layout.
    pixel_cells: std::cell::RefCell<PixelCells>,
}

type PixelCells = std::collections::HashMap<(usize, i64, i64), usize, std::hash::BuildHasherDefault<PixelCellHasher>>;

// These keys are derived integer coordinates, not author strings. A fixed polynomial
// avoids SipHash's byte processing; rotation distributes its high bits into bucket indices.
// Integer mixing: rust-lang/rustc-hash, f16da569fcb867fa1bc3b5dba18c9b8f7ca99483/src/lib.rs.
#[derive(Default)]
struct PixelCellHasher(u64);

impl PixelCellHasher {
    #[inline]
    fn word(&mut self, value: u64) {
        self.0 = self.0.wrapping_add(value).wrapping_mul(0xf135_7aea_2e62_a9c5);
    }
}

impl std::hash::Hasher for PixelCellHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.word(u64::from(byte));
        }
        self.word(bytes.len() as u64);
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.word(value as u64);
    }

    #[inline]
    fn write_i64(&mut self, value: i64) {
        self.word(value as u64);
    }
}

/// A distinct x of a line or area panel.
struct AtX {
    /// The series, value and x of every observation there, in series order.
    rows: Vec<(usize, Option<f64>, f64)>,
    /// Its heading and the visible series' lines; a stack's total is added per layout (TYP-36).
    readout: std::cell::OnceCell<Rc<Readout>>,
}

impl Read {
    /// Observation `k`'s value `v` at the author's precision.
    fn shown(&self, k: usize, v: f64, readout: &Format) -> f64 {
        let cell = &self.shown[k];
        if cell.get().is_nan() {
            cell.set(numbers::shown(v, readout));
        }
        cell.get()
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &[(Option<String>, Vec<usize>)]) -> Painted {
    let block = ctx.block;
    let kind = block.kind;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let color_field = block.encoding.color.clone();
    let self_colour = kind == ChartType::Bar && color_field.as_deref() == Some(x_field.as_str());

    let prepared = ctx.prepared;
    let data = prepared.cartesian.get_or_init(|| Data {
        keys: match &color_field {
            Some(field) => rows::distinct(ctx.rows, field),
            None => vec![block.y.label.clone().unwrap_or_else(|| y_field.clone())],
        },
        x_kind: rows::x_kind(kind, &x_field, ctx.rows),
        read: std::cell::OnceCell::new(),
    });
    ctx.set_series(&data.keys);
    let x_kind = data.x_kind;
    let width = ctx.opts.width;

    let read = data.read.get_or_init(|| {
        let (checks, notes) = (ctx.checks.len(), ctx.note.len());
        let parsed = parse(
            ctx,
            x_kind,
            &x_field,
            &y_field,
            color_field.as_deref(),
            self_colour,
            facets,
        );
        let shown = match &parsed {
            Ok(p) if matches!(kind, ChartType::Line | ChartType::Area) => {
                (0..p.obs.len()).map(|_| std::cell::Cell::new(f64::NAN)).collect()
            }
            _ => Vec::new(),
        };
        let drawn = parsed.as_ref().map_or_else(
            |_| Vec::new(),
            |p| {
                facets
                    .iter()
                    .zip(&p.failures)
                    .filter(|(_, f)| f.is_none())
                    .flat_map(|((_, rows), _)| rows.iter().copied())
                    .collect()
            },
        );
        Read {
            drawn,
            points: parsed
                .as_ref()
                .map_or(0, |p| p.drawn().filter(|o| o.y.is_some()).count()),
            parsed,
            checks: ctx.checks.split_off(checks),
            notes: ctx.note.split_off(notes),
            shown,
            panels: facets.iter().map(|_| std::cell::OnceCell::new()).collect(),
            extents: std::cell::RefCell::default(),
            summary: std::cell::OnceCell::new(),
        }
    });
    ctx.checks.extend(read.checks.iter().cloned());
    ctx.note.extend(read.notes.iter().cloned());
    let parsed = match &read.parsed {
        Ok(p) => p,
        Err(reason) => {
            let height = (width / kind.default_ratio(x_kind == XKind::Calendar)).max(MIN_PLOT + 40.0);
            let panel = super::failed_panel(ctx, Rect::new(0.0, 0.0, width, height), reason);
            return Painted {
                height,
                panels: vec![panel],
                figure_items: Vec::new(),
                legend: None,
                failure: Some(reason.clone()),
            };
        }
    };

    let multi = color_field.is_some() && !self_colour;
    let calendar = matches!(parsed.x, XModel::Calendar { .. });
    let ratio = block.style.ratio.unwrap_or_else(|| kind.default_ratio(calendar));
    let columns = super::facet_columns(facets.len(), width);
    let panel_width = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let title_room = if block.encoding.facet.is_some() {
        ctx.line_height() + 6.0
    } else {
        0.0
    };

    // Names at line ends when they fit; otherwise a legend (FEA-CH-27, FEA-CH-33).
    let line_like = matches!(kind, ChartType::Line | ChartType::Area);
    // Budget for every series, including hidden ones, so its restore control still has room.
    let visible_names: Vec<String> = ctx.series.iter().map(|s| s.name.clone()).collect();
    let end_width = visible_names.iter().map(|n| ctx.text_width(n)).fold(0.0, f64::max) + 10.0;
    let mut end_names = line_like && multi && end_width <= panel_width * 0.3;
    let horizontal = kind == ChartType::Bar && horizontal_bars(ctx, parsed, panel_width);

    // Panel height from the ratio, growing for horizontal bar rows (FEA-CH-65).
    let mut panel_height = panel_width / ratio + title_room;
    if horizontal && let XModel::Category { names, reader, .. } = &parsed.x {
        let row = ctx.line_height()
            * 1.6
            * if multi && !block.stack {
                parsed.groups.max(1) as f64 * 0.7
            } else {
                1.0
            };
        let narrow = panel_width < 480.0;
        let per_row = if narrow {
            row + name_lines(ctx, reader, panel_width - 12.0) as f64 * ctx.line_height()
        } else {
            row
        };
        let needed = names.len() as f64 * per_row + 3.0 * ctx.line_height() + title_room;
        panel_height = panel_height.max(needed);
    }
    let min_height = MIN_PLOT + 3.0 * ctx.line_height() + title_room;
    panel_height = panel_height.max(min_height);
    // Shared value range across panels unless scales are free (FEA-CH-36).
    let shared_y = derived_extent(ctx, read, parsed, 0, "shared", &read.drawn);
    ctx.fit = Fit {
        x: match &parsed.x {
            XModel::Calendar { extent, .. } | XModel::Numeric { extent, .. } => Some(*extent),
            XModel::Category { .. } => None,
        },
        y: Some(shared_y),
        size: block.encoding.size.as_ref().map(|_| bubble_max(ctx, parsed)),
        size_min: block.encoding.size.as_ref().and_then(|_| bubble_min(ctx, parsed)),
        color: None,
        categories: match &parsed.x {
            XModel::Category { names, .. } => names.clone(),
            _ => Vec::new(),
        },
        names: Vec::new(),
    };
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_height);
    // A names/legend decision starts from the all-shown range at this width. A hidden subset
    // may fall back when its own axis leaves no safe bands, but cannot turn an original legend
    // into names. Only data-space extents are replayed; pixel placement is measured afresh.
    // Record the all-shown data range even when this width cannot show direct names (TYP-36).
    let all_shared = if line_like && multi && !ctx.opts.hidden.is_empty() && !block.style.free_scales {
        ctx.derive(0, "cartesian.extent.shared", || {
            crate::derived::coarse(value_extent(ctx, parsed, &read.drawn, true))
        })
    } else {
        shared_y
    };
    let panel_extents: Vec<_> = facets
        .iter()
        .enumerate()
        .map(|(fi, (_, members))| {
            let current = if parsed.failures[fi].is_some() {
                shared_y
            } else if block.style.free_scales {
                derived_extent(ctx, read, parsed, fi, "own", members)
            } else {
                shared_y
            };
            let all = if !line_like || !multi || ctx.opts.hidden.is_empty() || parsed.failures[fi].is_some() {
                current
            } else if block.style.free_scales {
                ctx.derive(fi, "cartesian.extent.own", || {
                    crate::derived::coarse(value_extent(ctx, parsed, members, true))
                })
            } else {
                all_shared
            };
            (current, all)
        })
        .collect();
    if end_names {
        for (fi, ((_, members), frame)) in facets.iter().zip(&frames).enumerate() {
            if parsed.failures[fi].is_some() {
                continue;
            }
            let (current, all_extent) = panel_extents[fi];
            let members = read.panels[fi].get_or_init(|| Members::new(ctx, parsed, members));
            for (extent, all_shown) in
                std::iter::once((all_extent, true)).chain((!ctx.opts.hidden.is_empty()).then_some((current, false)))
            {
                let plan = panel_layout(ctx, parsed, members, *frame, title_room, extent, true);
                let plot = plan.fr.plot;
                if kind == ChartType::Line {
                    if !place::line_names_fit(ctx, plot) {
                        end_names = false;
                        break;
                    }
                    continue;
                }
                let ticks: Vec<_> = plan
                    .fr
                    .y
                    .ticks
                    .iter()
                    .zip(&plan.fr.y.labels)
                    .map(|(v, label)| {
                        ctx.text_mid(
                            label,
                            plot.x - LABEL_GAP,
                            plan.fr.y.map(*v, plot.bottom(), plot.y),
                            Anchor::End,
                            Face::Figures,
                            Role::AxisLabel,
                            ctx.style.ink,
                        )
                        .bounds
                    })
                    .collect();
                let values = area_name_values(
                    ctx,
                    read,
                    parsed,
                    members,
                    plan.xl.scale.as_ref().map(|x| x.domain),
                    all_shown,
                );
                let ends: Vec<_> = values
                    .into_iter()
                    .map(|(s, value, shown)| {
                        (
                            s,
                            plan.fr
                                .y
                                .map(value.clamp(plan.fr.y.lo, plan.fr.y.hi), plot.bottom(), plot.y),
                            shown,
                        )
                    })
                    .collect();
                let order = place::preview_name_order(ctx, fi, &ends, all_shown);
                if !place::area_names_fit(ctx, plot, &ticks, &ends, &order, area_name_limit(&plan.fr.y, plot)) {
                    end_names = false;
                    break;
                }
            }
            if !end_names {
                break;
            }
        }
    }
    let legend = (multi && !end_names).then(|| Legend {
        swatch: match kind {
            ChartType::Line => Swatch::Line,
            ChartType::Scatter => Swatch::Dot,
            _ => Swatch::Square,
        },
        entries: (0..ctx.series.len()).collect(),
        bottom: ctx.style.legend_bottom,
    });
    let mut panels = Vec::new();
    let mut figure_items = Vec::new();
    for (fi, (((name, members), frame), failure)) in facets.iter().zip(&frames).zip(&parsed.failures).enumerate() {
        if let Some(reason) = failure {
            panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
            continue;
        }
        let y_extent = panel_extents[fi].0;
        let panel_members = read.panels[fi].get_or_init(|| Members::new(ctx, parsed, members));
        let mut panel = paint_panel(
            ctx,
            read,
            parsed,
            panel_members,
            *frame,
            title_room,
            y_extent,
            end_names,
            horizontal,
            fi,
        );
        if let Some(name) = name {
            let title = ctx.text(
                name,
                frame.x,
                frame.y + ctx.style.font.baseline_offset(ctx.size()),
                Anchor::Start,
                Face::Data,
                Role::PanelTitle,
                ctx.style.ink,
            );
            panel
                .items
                .push(Item::Text(crate::geometry::Text { weight: 600, ..title }));
        }
        panels.push(panel);
    }
    if block.style.free_scales && facets.len() > 1 {
        ctx.note.push("Each panel has its own value scale".into());
    }
    ctx.summary = read.summary.get_or_init(|| summarize(ctx, parsed)).clone();
    let failed = parsed.failures.iter().filter(|f| f.is_some()).count();
    if failed > 0 {
        ctx.summary
            .push_str(&format!(" {failed} of {} panels could not be drawn.", facets.len()));
    }
    let mut height = super::grid_height(facets.len(), columns, panel_height, super::FACET_GAP_Y);
    // The bubble size reference under the plot (FEA-CH-07).
    if kind == ChartType::Scatter && block.encoding.size.is_some() {
        let (items, key_height) = bubble_key(ctx, parsed, height);
        figure_items.extend(items);
        height += key_height;
    }
    Painted {
        height,
        panels,
        figure_items,
        legend,
        failure: None,
    }
}

/// The accessible one-sentence summary per type (FEA-CH-55), at the author's precision.
fn summarize(ctx: &Ctx, p: &Parsed) -> String {
    let block = ctx.block;
    let f = block.label_format(&block.y, ctx.locale);
    let visible = |o: &&Obs| !ctx.is_hidden(o.series);
    match block.kind {
        ChartType::Line | ChartType::Area => {
            let xs: Vec<f64> = p.drawn().map(|o| o.x).collect();
            let lo = xs.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let range = if lo.is_finite() {
                format!("from {} to {}", heading_for_x(ctx, p, lo), heading_for_x(ctx, p, hi))
            } else {
                String::new()
            };
            let lasts: Vec<String> = (0..ctx.series.len())
                .filter(|s| !ctx.is_hidden(*s))
                .filter_map(|s| {
                    let last = p
                        .obs
                        .iter()
                        .filter(|o| o.series == s && o.y.is_some())
                        .max_by(|a, b| a.x.total_cmp(&b.x))?;
                    Some(format!(
                        "{} ends at {}",
                        series_label(ctx, p, s),
                        numbers::value(last.y?, &f)
                    ))
                })
                .collect();
            match lasts.len() {
                0 => "No values to show.".into(),
                n if range.is_empty() => format!("{n} series: {}.", lasts.join("; ")),
                n => format!("{n} series, {range}: {}.", lasts.join("; ")),
            }
        }
        ChartType::Bar => {
            let XModel::Category { names, .. } = &p.x else {
                return String::new();
            };
            let valued: Vec<&Obs> = p.drawn().filter(visible).filter(|o| o.y.is_some()).collect();
            let mut shown: Vec<i64> = p.drawn().map(|o| o.x as i64).collect();
            shown.sort_unstable();
            shown.dedup();
            let categories = shown.len();
            let hi = valued
                .iter()
                .max_by(|a, b| a.y.unwrap_or(0.0).total_cmp(&b.y.unwrap_or(0.0)));
            let lo = valued
                .iter()
                .min_by(|a, b| a.y.unwrap_or(0.0).total_cmp(&b.y.unwrap_or(0.0)));
            match (hi, lo) {
                (Some(h), Some(l)) => format!(
                    "{}; highest {} at {}, lowest {} at {}.",
                    super::count(categories, "category", "categories"),
                    names[h.x as usize],
                    numbers::value(h.y.unwrap_or(0.0), &f),
                    names[l.x as usize],
                    numbers::value(l.y.unwrap_or(0.0), &f)
                ),
                _ => format!("{} without values.", super::count(categories, "category", "categories")),
            }
        }
        ChartType::Scatter => {
            let pts: Vec<&Obs> = p.drawn().filter(visible).filter(|o| o.y.is_some()).collect();
            let fx = block.label_format(&block.x, ctx.locale);
            let ext = |v: Vec<f64>| {
                (
                    v.iter().copied().fold(f64::INFINITY, f64::min),
                    v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                )
            };
            let (x0, x1) = ext(pts.iter().map(|o| o.x).collect());
            let (y0, y1) = ext(pts.iter().filter_map(|o| o.y).collect());
            if pts.is_empty() {
                return "No points.".into();
            }
            format!(
                "{}; x from {} to {}, y from {} to {}.",
                super::count(pts.len(), "point", "points"),
                numbers::value(x0, &fx),
                numbers::value(x1, &fx),
                numbers::value(y0, &f),
                numbers::value(y1, &f)
            )
        }
        _ => String::new(),
    }
}

/// Most lines a name takes above its bar; a name needing more is reported unfit (FEA-CH-26).
const NAME_LINES_ABOVE: usize = 4;
/// Room between the axis line and a category name set above its bar.
const NAME_INSET: f64 = 6.0;
/// Shape markers on line series that need a second cue (STY-22).
const MARKER_RADIUS: f64 = 3.5;
/// A break narrower than this many pixels is marked on the rug: a 2.5 px line hides it.
const RUG_BELOW: f64 = 3.0;
const MARKER_STROKE: f64 = 1.5;

/// Lines the longest category name needs above its bar at `width`.
fn name_lines(ctx: &Ctx, reader: &[String], width: f64) -> usize {
    let font = &ctx.style.font;
    reader
        .iter()
        .map(|n| font.wrap(n, ctx.size(), width.max(1.0), NAME_LINES_ABOVE).len())
        .max()
        .unwrap_or(1)
        .clamp(1, NAME_LINES_ABOVE)
}

/// Whether nominal bars turn horizontal because their names do not fit (FEA-CH-06, FEA-CH-26).
fn horizontal_bars(ctx: &Ctx, p: &Parsed, panel_width: f64) -> bool {
    match ctx.block.style.orientation {
        Orientation::Horizontal => true,
        Orientation::Vertical => false,
        Orientation::Auto => {
            let XModel::Category { names, reader, ordered } = &p.x else {
                return false;
            };
            if ordered.is_some() {
                return false;
            }
            let plot = panel_width - 60.0;
            let band = plot / names.len().max(1) as f64;
            matches!(
                axis::category_layout(reader, None, band, &ctx.style.font, ctx.size()),
                CategoryLayout::Unfit { .. }
            )
        }
    }
}

fn parse(
    ctx: &mut Ctx,
    x_kind: XKind,
    x_field: &str,
    y_field: &str,
    color: Option<&str>,
    self_colour: bool,
    facets: &[(Option<String>, Vec<usize>)],
) -> Result<Parsed, String> {
    let block = ctx.block;
    let kind = block.kind;
    let mut obs = Vec::new();
    let mut excluded = 0usize;
    // Rows whose value is missing: drawn as gaps and counted (TYP-91, FEA-CH-38).
    let mut gaps = 0usize;
    let facet_of = super::facet_index(facets, ctx.rows.len());
    let mut failures: Vec<Option<String>> = vec![None; facets.len().max(1)];
    let mut precision = Precision::Year;
    // A playback keeps every frame's categories on the axis in their order (FEA-BR-31).
    let categories: Vec<String> = if x_kind == XKind::Category {
        let mut names = ctx.wanted().categories;
        for name in rows::distinct(ctx.rows, x_field) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    } else {
        Vec::new()
    };
    for (i, row) in ctx.rows.iter().enumerate() {
        // Rows in no facet panel are disclosed once by the layout.
        let Some(panel) = facet_of[i] else { continue };
        let series = match color {
            Some(field) => match rows::text(row, field) {
                Some(key) => ctx.series.iter().position(|s| s.key == key).unwrap_or(0),
                None => {
                    excluded += 1;
                    continue;
                }
            },
            None => 0,
        };
        let x = match x_kind {
            XKind::Calendar => {
                let Some((ms, p)) = rows::cell(row, x_field).as_str().and_then(time::parse_iso) else {
                    excluded += 1;
                    continue;
                };
                precision = precision.max(p);
                ms as f64
            }
            XKind::Numeric => match rows::num(row, x_field) {
                Num::Value(v) => v,
                Num::Missing => {
                    excluded += 1;
                    continue;
                }
                Num::Underflow(text) => {
                    ctx.underflow(i, x_field, &text);
                    // Report a y value in the same row too, so one pass names every one.
                    if let Num::Underflow(y) = rows::num(row, y_field) {
                        ctx.underflow(i, y_field, &y);
                    }
                    failures[panel]
                        .get_or_insert_with(|| "Cannot draw this chart · An x value is too small to represent".into());
                    continue;
                }
                Num::Invalid(text) => {
                    let path = super::Ctx::row_path(i, x_field);
                    ctx.check(
                        "non-numeric",
                        Level::Error,
                        format!("Row {} has x value '{text}', which is not a number", i + 1),
                        Some("Write plain numbers such as 1234.5, without grouping."),
                        Some(path),
                        None,
                        json!({"row": i, "value": text}),
                    );
                    failures[panel].get_or_insert_with(|| "Cannot draw this chart · An x value is not a number".into());
                    continue;
                }
            },
            XKind::Category => match rows::text(row, x_field) {
                Some(name) => categories.iter().position(|c| *c == name).unwrap_or(0) as f64,
                None => {
                    excluded += 1;
                    continue;
                }
            },
        };
        let y = match rows::num(row, y_field) {
            Num::Value(v) => Some(v),
            Num::Missing => {
                gaps += 1;
                None
            }
            Num::Invalid(text) => {
                ctx.check("non-numeric", Level::Error, format!("Row {} has {y_field} '{text}', which is not a number", i + 1), Some("Write plain numbers such as 1234.5, without grouping or units; missing values may be empty or NA."), Some(super::Ctx::row_path(i, y_field)), None, json!({"row": i, "value": text}));
                failures[panel]
                    .get_or_insert_with(|| format!("Cannot draw this chart · {y_field} has text that is not a number"));
                continue;
            }
            Num::Underflow(text) => {
                ctx.underflow(i, y_field, &text);
                failures[panel].get_or_insert_with(|| {
                    format!("Cannot draw this chart · {y_field} has a value too small to represent")
                });
                continue;
            }
        };
        let size = match &block.encoding.size {
            Some(field) => match rows::num(row, field) {
                Num::Value(v) if v >= 0.0 => Some(v),
                Num::Value(v) => {
                    ctx.check(
                        "invalid-bubble-size",
                        Level::Error,
                        format!("Row {} has a negative bubble size {v}; it is left out", i + 1),
                        Some("Bubble sizes must be zero or positive."),
                        Some(super::Ctx::row_path(i, field)),
                        None,
                        json!({"row": i}),
                    );
                    continue;
                }
                Num::Missing => {
                    ctx.check(
                        "missing-bubble-size",
                        Level::Warning,
                        format!("Row {} has no bubble size; it is left out", i + 1),
                        None,
                        Some(super::Ctx::row_path(i, field)),
                        None,
                        json!({"row": i}),
                    );
                    continue;
                }
                Num::Underflow(t) => {
                    ctx.underflow(i, field, &t);
                    continue;
                }
                Num::Invalid(t) => {
                    ctx.check(
                        "invalid-bubble-size",
                        Level::Error,
                        format!(
                            "Row {} has bubble size '{t}', which is not a number; it is left out",
                            i + 1
                        ),
                        None,
                        Some(super::Ctx::row_path(i, field)),
                        None,
                        json!({"row": i}),
                    );
                    continue;
                }
            },
            None => None,
        };
        obs.push(Obs {
            row: i,
            series,
            x,
            y,
            size,
        });
    }
    if gaps > 0 {
        ctx.check(
            "missing-observations",
            Level::Info,
            format!(
                "{} no {y_field} value and {} drawn as {}",
                super::count(gaps, "row has", "rows have"),
                super::verb(gaps, "is", "are"),
                super::verb(gaps, "a gap", "gaps")
            ),
            None,
            None,
            None,
            json!({"count": gaps}),
        );
    }
    if excluded > 0 {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} without an x or group value {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            None,
            None,
            None,
            json!({"count": excluded}),
        );
        ctx.note.push(format!(
            "{} without a position {} not shown",
            super::count(excluded, "row", "rows"),
            super::verb(excluded, "is", "are")
        ));
    }
    let groups = if self_colour { 1 } else { ctx.series.len() };
    // Slots repeat only within one facet panel.
    let x = match x_kind {
        XKind::Calendar => {
            // Repeated timestamps within one series cannot draw (FEA-CH-15).
            let mut seen = std::collections::HashSet::new();
            let repeats: Vec<usize> = obs
                .iter()
                .enumerate()
                .filter(|(_, o)| !seen.insert((facet_of[o.row], o.series, o.x as i64)))
                .map(|(k, _)| k)
                .collect();
            for k in repeats {
                let o = &obs[k];
                let Some(panel) = facet_of[o.row] else { continue };
                if failures[panel].is_some() {
                    continue;
                }
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!(
                        "Row {} repeats a timestamp in series '{}'",
                        o.row + 1,
                        ctx.series[o.series].name
                    ),
                    Some("Give each series one row per timestamp."),
                    Some(super::Ctx::row_path(o.row, x_field)),
                    None,
                    json!({"row": o.row}),
                );
                failures[panel] = Some("Cannot draw this time series · Repeated timestamps".into());
            }
            let mut xs: Vec<f64> = obs.iter().map(|o| o.x).collect();
            xs.sort_by(f64::total_cmp);
            xs.dedup();
            let spacing = typical_spacing(&xs);
            let extent = [xs.first().copied().unwrap_or(0.0), xs.last().copied().unwrap_or(1.0)];
            XModel::Calendar {
                precision,
                spacing: spacing as i64,
                extent: Fit::widen(extent, ctx.wanted().x),
            }
        }
        XKind::Numeric => {
            let xs: Vec<f64> = obs.iter().map(|o| o.x).collect();
            let lo = xs.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let years = match block.x.format {
                Some(Kind::Year) => true,
                Some(_) => false,
                None => numbers::looks_like_years(&xs),
            };
            let extent = if lo.is_finite() { [lo, hi] } else { [0.0, 1.0] };
            XModel::Numeric {
                extent: Fit::widen(extent, ctx.wanted().x),
                years,
            }
        }
        XKind::Category => {
            let ordered = axis::ordered(&categories, ctx.locale);
            let reader = ordered.as_ref().map_or_else(|| categories.clone(), |o| o.text.clone());
            XModel::Category {
                names: categories,
                reader,
                ordered,
            }
        }
    };
    // Bars: one value per slot (FEA-CH-06).
    if kind == ChartType::Bar {
        let mut slots = std::collections::HashMap::new();
        for o in &obs {
            *slots.entry((facet_of[o.row], o.x as i64, o.series)).or_insert(0usize) += 1;
        }
        let repeated: Vec<_> = slots.iter().filter(|(_, n)| **n > 1).collect();
        if !repeated.is_empty() {
            let XModel::Category { names, .. } = &x else {
                unreachable!("bars are categorical")
            };
            let facet_name = |f: &Option<usize>| {
                block
                    .encoding
                    .facet
                    .as_deref()
                    .zip(f.and_then(|f| obs.iter().find(|o| facet_of[o.row] == Some(f))))
                    .and_then(|(field, o)| rows::text(&ctx.rows[o.row], field))
            };
            let details: Vec<Value> = repeated
                .iter()
                .map(|((facet, cat, s), n)| json!({"facet": facet_name(facet), "category": names[*cat as usize], "series": ctx.series[*s].name, "count": n}))
                .collect();
            let failing: Vec<usize> = repeated.iter().filter_map(|((f, _, _), _)| *f).collect();
            ctx.check(
                "ambiguous-bar-slot",
                Level::Error,
                format!("{} bar slots have more than one row", repeated.len()),
                Some("Aggregate the rows so each category and series has one value."),
                Some("chart.encoding.y".into()),
                None,
                json!({"slots": details}),
            );
            for panel in failing {
                failures[panel].get_or_insert_with(|| "Cannot draw these bars · Repeated observations".into());
            }
        }
    }
    if kind == ChartType::Scatter {
        for (panel, failure) in failures.iter_mut().enumerate() {
            let mut members = obs.iter().filter(|o| facet_of[o.row] == Some(panel)).peekable();
            if members.peek().is_some() && members.all(|o| o.y.is_none()) {
                failure.get_or_insert_with(|| "Cannot draw this chart · No numeric y values".into());
            }
        }
    }
    // Stack totals follow the pie rule: a total too large to represent cannot draw (FEA-CH-70).
    if block.stack {
        let mut totals: std::collections::HashMap<(Option<usize>, i64, bool), f64> = Default::default();
        for o in &obs {
            if let Some(y) = o.y {
                *totals
                    .entry((facet_of[o.row], o.x.to_bits() as i64, y >= 0.0))
                    .or_default() += y;
            }
        }
        let mut overflowing: Vec<usize> = totals
            .iter()
            .filter(|(_, t)| !t.is_finite())
            .filter_map(|((f, _, _), _)| *f)
            .collect();
        overflowing.sort_unstable();
        overflowing.dedup();
        if !overflowing.is_empty() {
            ctx.check(
                "value-overflow",
                Level::Error,
                "A stack total is too large to represent",
                Some("Scale the values down, for example to thousands or millions."),
                Some("chart.encoding.y".into()),
                None,
                json!({"panels": overflowing}),
            );
        }
        for panel in overflowing {
            failures[panel].get_or_insert_with(|| "Cannot draw these stacks · A stack total is too large".into());
        }
    }
    // The whole chart fails when no panel can draw.
    if failures.iter().all(Option::is_some) {
        return Err(failures.into_iter().flatten().next().unwrap_or_default());
    }
    Ok(Parsed {
        obs,
        x,
        groups,
        self_colour,
        failures,
        panel_of: facet_of,
    })
}

/// The median distance between consecutive distinct positions.
fn typical_spacing(xs: &[f64]) -> f64 {
    let mut d: Vec<f64> = xs.windows(2).map(|w| w[1] - w[0]).filter(|d| *d > 0.0).collect();
    if d.is_empty() {
        return time::MS_DAY as f64;
    }
    d.sort_by(f64::total_cmp);
    d[d.len() / 2]
}

/// Smallest and largest value the value axis must show for visible series and their stack
/// totals, independent of reader zoom (TYP-36).
fn value_extent(ctx: &Ctx, p: &Parsed, members: &[usize], all_shown: bool) -> [f64; 2] {
    let block = ctx.block;
    let member: std::collections::HashSet<usize> = members.iter().copied().collect();
    let included = |o: &&Obs| member.contains(&o.row) && (all_shown || !ctx.is_hidden(o.series));
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    if block.stack {
        let mut pos: std::collections::HashMap<i64, f64> = Default::default();
        let mut neg: std::collections::HashMap<i64, f64> = Default::default();
        for o in p.obs.iter().filter(included) {
            if let Some(y) = o.y {
                let key = (o.x * 1000.0) as i64;
                if y >= 0.0 {
                    *pos.entry(key).or_default() += y
                } else {
                    *neg.entry(key).or_default() += y
                }
            }
        }
        for v in pos.values().chain(neg.values()) {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
    } else {
        for y in p.obs.iter().filter(included).filter_map(|o| o.y) {
            lo = lo.min(y);
            hi = hi.max(y);
        }
    }
    let wanted = ctx.wanted().y;
    if !lo.is_finite() {
        return wanted.unwrap_or([0.0, 1.0]);
    }
    Fit::widen([lo, hi], wanted)
}

/// Record the visible-series extent independently of x zoom. The runtime derives missing
/// reader subsets from shown rows and replays that same extent for grid marks (TYP-36).
fn derived_extent(ctx: &Ctx, read: &Read, p: &Parsed, panel: usize, name: &'static str, members: &[usize]) -> [f64; 2] {
    let key = format!("cartesian.extent.{name}");
    ctx.derive_visible(panel, &key, || {
        let known = read
            .extents
            .borrow()
            .iter()
            .find(|e| e.0 == panel && e.1 == name)
            .map(|e| e.2);
        known.unwrap_or_else(|| {
            let extent = crate::derived::coarse(value_extent(ctx, p, members, false));
            read.extents.borrow_mut().push((panel, name, extent));
            extent
        })
    })
}

impl Members {
    /// The observations of the panel whose rows are `members`, ordered and given their readouts
    /// once per data state.
    fn new(ctx: &Ctx, p: &Parsed, members: &[usize]) -> Members {
        let member: std::collections::HashSet<usize> = members.iter().copied().collect();
        let obs: Vec<usize> = (0..p.obs.len()).filter(|&k| member.contains(&p.obs[k].row)).collect();
        let lo = obs.iter().map(|&k| p.obs[k].x).fold(f64::INFINITY, f64::min);
        let hi = obs.iter().map(|&k| p.obs[k].x).fold(f64::NEG_INFINITY, f64::max);
        let mut out = Members {
            obs,
            x_extent: if lo.is_finite() { [lo, hi] } else { [0.0, 1.0] },
            x_ordered: false,
            series: Vec::new(),
            at_x: Vec::new(),
            kinds: Vec::new(),
            readouts: Vec::new(),
            pixel_cells: std::cell::RefCell::default(),
        };
        match ctx.block.kind {
            ChartType::Line | ChartType::Area => out.read_lines(ctx, p),
            ChartType::Scatter => out.read_points(p),
            _ => {}
        }
        out
    }

    /// Each series' observations in x order, and every distinct x with the observations there.
    fn read_lines(&mut self, ctx: &Ctx, p: &Parsed) {
        let key = |x: f64| (x * 1000.0).round() as i64;
        let mut by_x: std::collections::BTreeMap<i64, Vec<(usize, Option<f64>, f64)>> = Default::default();
        for s in 0..ctx.series.len() {
            let mut pts: Vec<usize> = self.obs.iter().copied().filter(|&k| p.obs[k].series == s).collect();
            pts.sort_by(|&a, &b| p.obs[a].x.total_cmp(&p.obs[b].x));
            for &k in &pts {
                let o = &p.obs[k];
                by_x.entry(key(o.x)).or_default().push((s, o.y, o.x));
            }
            self.series.push(pts);
        }
        self.at_x = by_x
            .into_values()
            .map(|rows| AtX {
                rows,
                readout: std::cell::OnceCell::new(),
            })
            .collect();
        // The value each series' automatic labels name, found once for every zoom: the last
        // lowest value and the first highest, as the readout reads them (FEA-CH-28).
        self.kinds = vec![[None; 4]; ctx.series.len()];
        let mut extremes = vec![[f64::INFINITY, f64::NEG_INFINITY]; ctx.series.len()];
        for (i, a) in self.at_x.iter().enumerate() {
            for (s, kinds) in self.kinds.iter_mut().enumerate() {
                let Some(v) = a.rows.iter().find(|r| r.0 == s).and_then(|r| r.1) else {
                    continue;
                };
                kinds[0] = Some(i);
                kinds[1].get_or_insert(i);
                if v <= extremes[s][0] {
                    extremes[s][0] = v;
                    kinds[2] = Some(i);
                }
                if v > extremes[s][1] {
                    extremes[s][1] = v;
                    kinds[3] = Some(i);
                }
            }
        }
    }

    fn read_points(&mut self, p: &Parsed) {
        self.x_ordered = self.obs.windows(2).all(|w| p.obs[w[0]].x <= p.obs[w[1]].x);
        self.readouts = self.obs.iter().map(|_| std::cell::OnceCell::new()).collect();
    }
}

impl AtX {
    fn x(&self) -> f64 {
        self.rows[0].2
    }

    /// The visible series' values here, in legend order: the first observation of each.
    fn values<'a>(&'a self, ctx: &'a Ctx) -> impl Iterator<Item = (usize, f64)> + 'a {
        (0..ctx.series.len())
            .filter(|&s| !ctx.is_hidden(s))
            .filter_map(|s| self.rows.iter().find(|r| r.0 == s))
            .filter_map(|r| r.1.map(|v| (r.0, v)))
    }

    /// The sum of the visible series' values here, a stack's total.
    fn total(&self, ctx: &Ctx) -> f64 {
        self.rows
            .iter()
            .filter(|r| !ctx.is_hidden(r.0))
            .filter_map(|r| r.1)
            .sum()
    }

    /// The heading and one line per visible series in legend order (FEA-CH-41).
    fn readout(&self, ctx: &Ctx, p: &Parsed, readout: &Format) -> &Rc<Readout> {
        self.readout.get_or_init(|| {
            let n_series = ctx.series.len();
            let mut lines = Vec::new();
            for s in 0..n_series {
                if ctx.is_hidden(s) {
                    continue;
                }
                let value = match self.rows.iter().find(|r| r.0 == s) {
                    Some((_, Some(v), _)) => numbers::value(*v, readout),
                    Some((_, None, _)) => "No observation".into(),
                    None if n_series > 1 => "No observation".into(),
                    None => continue,
                };
                lines.push(ReadoutLine {
                    name: series_label(ctx, p, s),
                    value,
                    series: Some(s),
                    names_series: ctx.block.encoding.color.is_some() && !p.self_colour,
                    derived: false,
                });
            }
            Rc::new(Readout {
                heading: heading_for_x(ctx, p, self.x()),
                lines,
            })
        })
    }
}

/// A scatter point's readout: x, y, size, colour and detail fields, under its label or number.
/// `formats` are the x, y and size readout formats.
fn point_readout(ctx: &Ctx, o: &Obs, v: f64, formats: [&Format; 3]) -> Readout {
    let block = ctx.block;
    let [x_readout, y_readout, size_readout] = formats;
    let label_on_x = block.encoding.label.is_some() && block.encoding.label == block.encoding.x;
    let label_on_y = block.encoding.label.is_some() && block.encoding.label == block.encoding.y;
    let mut lines = vec![
        ReadoutLine {
            name: block
                .x
                .label
                .clone()
                .or_else(|| block.encoding.x.as_deref().map(readable_name))
                .unwrap_or_default(),
            value: numbers::value(o.x, x_readout),
            series: None,
            names_series: false,
            derived: false,
        },
        ReadoutLine {
            name: block
                .y
                .label
                .clone()
                .or_else(|| block.encoding.y.as_deref().map(readable_name))
                .unwrap_or_default(),
            value: numbers::value(v, y_readout),
            series: Some(o.series),
            names_series: false,
            derived: false,
        },
    ];
    let heading = if label_on_x {
        format!("{} {}", lines[0].name, lines[0].value)
    } else if label_on_y {
        format!("{} {}", lines[1].name, lines[1].value)
    } else {
        block
            .encoding
            .label
            .as_ref()
            .and_then(|f| rows::text(&ctx.rows[o.row], f))
            .or_else(|| block.encoding.color.as_ref().map(|_| ctx.series[o.series].name.clone()))
            .unwrap_or_else(|| format!("Point {}", o.row + 1))
    };
    if label_on_y {
        lines.remove(1);
    }
    if label_on_x {
        lines.remove(0);
    }
    if let (Some(field), Some(s)) = (&block.encoding.size, o.size) {
        lines.push(ReadoutLine {
            name: block.size_axis().label.clone().unwrap_or_else(|| readable_name(field)),
            value: numbers::value(s, size_readout),
            series: None,
            names_series: false,
            derived: false,
        });
    }
    if block.encoding.color.is_some()
        && block.encoding.color != block.encoding.label
        && heading != ctx.series[o.series].name
    {
        lines.push(ReadoutLine {
            name: block.encoding.color.as_deref().map(readable_name).unwrap_or_default(),
            value: ctx.series[o.series].name.clone(),
            series: Some(o.series),
            names_series: true,
            derived: false,
        });
    }
    for field in &block.encoding.detail {
        if let Some(t) = rows::text(&ctx.rows[o.row], field) {
            let value = match rows::num(&ctx.rows[o.row], field) {
                Num::Value(n) => numbers::value(n, size_readout),
                _ => t,
            };
            if block.encoding.label.as_ref() != Some(field) && value != heading {
                lines.push(ReadoutLine {
                    name: readable_name(field),
                    value,
                    series: None,
                    names_series: false,
                    derived: false,
                });
            }
        }
    }
    Readout { heading, lines }
}

/// The id of the panel whose observations are `obs`, for keys of derived structures.
fn panel_of(obs: &[&Obs]) -> usize {
    obs.first().map_or(0, |o| o.row)
}

/// The visible x window: the zoom, clipped to the data, or everything.
fn x_window(ctx: &Ctx, extent: [f64; 2]) -> [f64; 2] {
    match ctx.opts.zoom {
        Some([a, b]) if a < b => [a.max(extent[0]), b.min(extent[1])],
        _ => extent,
    }
}

#[derive(Clone, Copy)]
enum XMapping {
    Calendar { lo: f64, span: f64, start: f64, width: f64 },
    Numeric(crate::scale::Linear),
    Category { start: f64, band: f64 },
}

impl XMapping {
    #[inline]
    fn at(self, value: f64) -> f64 {
        match self {
            Self::Calendar { lo, span, start, width } => start + (value - lo) / span * width,
            Self::Numeric(scale) => scale.map(value),
            Self::Category { start, band } => start + (value + 0.5) * band,
        }
    }
}

struct XLayout {
    /// Pixel position of an x value.
    mapping: XMapping,
    ticks: Vec<(f64, Vec<String>)>,
    /// Category band width, for bars.
    band: f64,
    scale: Option<XScale>,
    unfit: Vec<String>,
    thinned: Option<String>,
}

impl XLayout {
    #[inline]
    fn map(&self, value: f64) -> f64 {
        self.mapping.at(value)
    }
}

fn x_layout(ctx: &Ctx, p: &Parsed, plot: Rect, scatter_extent: Option<[f64; 2]>) -> XLayout {
    let size = ctx.size();
    let font = &ctx.style.font;
    match &p.x {
        XModel::Calendar {
            precision,
            spacing,
            extent,
        } => {
            let [a, b] = x_window(ctx, *extent);
            let b = if b > a { b } else { a + time::MS_DAY as f64 };
            let axis: CalendarAxis = axis::calendar_axis(
                a as i64,
                b as i64,
                *spacing,
                *precision,
                plot.w,
                font,
                size,
                ctx.calendar_locale(),
            );
            let (x0, w) = (plot.x, plot.w);
            let map = XMapping::Calendar {
                lo: a,
                span: b - a,
                start: x0,
                width: w,
            };
            let ticks = axis
                .ticks
                .iter()
                .zip(&axis.labels)
                .map(|(t, l)| (map.at(*t as f64), vec![l.clone()]))
                .collect();
            XLayout {
                mapping: map,
                ticks,
                band: 0.0,
                scale: Some(XScale {
                    domain: [a, b],
                    extent: *extent,
                    range: [plot.x, plot.right()],
                    calendar: true,
                }),
                unfit: vec![],
                thinned: None,
            }
        }
        XModel::Numeric { extent, years } => {
            let format = Format {
                kind: if *years {
                    Kind::Year
                } else {
                    ctx.block.x.format.unwrap_or(Kind::Auto)
                },
                unit: ctx.block.x.unit.clone(),
                digits: ctx.block.x.digits.unwrap_or(ctx.block.digits),
                locale: ctx.locale,
            };
            // A scatter plot's whole view is its padded extent on round ticks; that view is
            // also its zoom limit, so a zoom inside it is drawn exactly (FEA-CH-49).
            let room = point_room(ctx);
            let whole = scatter_extent.map(|padded| {
                let axis = axis::value_axis_room(
                    &ValueSpec {
                        data: padded,
                        zero: Zero::None,
                        min: None,
                        max: None,
                        length: plot.w,
                        horizontal: true,
                        format: &format,
                        font: &ctx.style.figures,
                        size,
                    },
                    room,
                );
                [axis.lo, axis.hi]
            });
            let limit = whole.unwrap_or(*extent);
            let (data, fixed) = match (scatter_extent, ctx.opts.zoom) {
                (Some(padded), Some([a, b])) if a < b => {
                    let window = [a.max(limit[0]), b.min(limit[1])];
                    if window[1] > window[0] && (window[0] > limit[0] || window[1] < limit[1]) {
                        (window, true)
                    } else {
                        (padded, false)
                    }
                }
                (Some(padded), _) => (padded, false),
                (None, _) => (x_window(ctx, *extent), true),
            };
            let spec = ValueSpec {
                data,
                zero: Zero::None,
                min: if fixed { Some(data[0]) } else { None },
                // A single x gets a sliver of width; tiny but distinct extents keep theirs.
                max: if fixed {
                    Some(if data[1] > data[0] {
                        data[1]
                    } else {
                        data[0] + data[0].abs().max(1.0) * 1e-9
                    })
                } else {
                    None
                },
                length: plot.w,
                horizontal: true,
                format: &format,
                font: &ctx.style.figures,
                size,
            };
            let axis: ValueAxis = axis::value_axis_room(&spec, if scatter_extent.is_some() { room } else { 0.0 });
            let (lo, hi) = (axis.lo, axis.hi);
            let (x0, w) = (plot.x, plot.w);
            // Halves before subtracting where the span itself would overflow (±1e308).
            let x = crate::scale::Linear::new((lo, hi), (x0, x0 + w));
            let map = XMapping::Numeric(x);
            let ticks = axis
                .ticks
                .iter()
                .zip(&axis.labels)
                .map(|(t, l)| (map.at(*t), vec![l.clone()]))
                .collect();
            XLayout {
                mapping: map,
                ticks,
                band: 0.0,
                scale: Some(XScale {
                    domain: [lo, hi],
                    extent: limit,
                    range: [plot.x, plot.right()],
                    calendar: false,
                }),
                unfit: vec![],
                thinned: None,
            }
        }
        XModel::Category { names, reader, ordered } => {
            let n = names.len().max(1);
            let band = plot.w / n as f64;
            let x0 = plot.x;
            let map = XMapping::Category { start: x0, band };
            let layout = axis::category_layout_with(reader, ordered.as_ref(), band, font, size, true);
            let mut unfit = Vec::new();
            let mut thinned = None;
            let lines: Vec<Vec<String>> = match layout {
                CategoryLayout::OneLine => reader.iter().map(|r| vec![r.clone()]).collect(),
                CategoryLayout::Wrapped(lines) => lines,
                CategoryLayout::Thinned { every, lines } => {
                    thinned = Some(format!(
                        "every {every} {}s",
                        ordered.as_ref().map_or("name", |o| o.period.noun())
                    ));
                    lines
                }
                CategoryLayout::Unfit { lines, unfit: which } => {
                    unfit = which.iter().map(|&i| reader[i].clone()).collect();
                    lines
                }
            };
            let ticks = lines
                .into_iter()
                .enumerate()
                .map(|(i, l)| (map.at(i as f64), l))
                .collect();
            XLayout {
                mapping: map,
                ticks,
                band,
                scale: None,
                unfit,
                thinned,
            }
        }
    }
}

/// Pixel positions shared by the painters of one panel.
struct Frame {
    plot: Rect,
    y: ValueAxis,
}

struct PanelLayout {
    fr: Frame,
    xl: XLayout,
    x_title_lines: Vec<String>,
    bottom_rows: f64,
    top: f64,
}

/// Measure axes and name margins before painting data, so names can fall back without repainting marks.
fn panel_layout(
    ctx: &Ctx,
    p: &Parsed,
    members: &Members,
    frame: Rect,
    title_room: f64,
    y_extent: [f64; 2],
    end_names: bool,
) -> PanelLayout {
    let block = ctx.block;
    let kind = block.kind;
    let size = ctx.size();
    let lh = ctx.line_height();
    let font = ctx.style.font.clone();
    let y_format = block.label_format(&block.y, ctx.locale);
    let zero = match kind {
        ChartType::Line => Zero::Soft,
        ChartType::Scatter => Zero::None,
        _ => Zero::Include,
    };
    let top = frame.y + title_room + if block.y.label.is_some() { lh + 4.0 } else { 0.0 } + lh / 2.0;
    // A long x title wraps rather than leave the chart (FEA-CH-20).
    let x_title_lines: Vec<String> = block
        .x
        .label
        .as_deref()
        .map(|l| font.wrap(l, ctx.size(), (frame.w - 24.0).max(40.0), 3))
        .unwrap_or_default();
    let x_title = if x_title_lines.is_empty() {
        0.0
    } else {
        x_title_lines.len() as f64 * lh + 2.0
    };
    let mut bottom_rows = 1.0;
    let end_gutter = if end_names {
        ctx.series.iter().map(|s| ctx.text_width(&s.name)).fold(0.0, f64::max) + 18.0
    } else {
        0.0
    };
    let scatter_extent =
        (kind == ChartType::Scatter).then(|| pad_extent(Fit::widen(members.x_extent, ctx.wanted().x), 0.04));
    let mut layout: Option<(Frame, XLayout)> = None;
    let mut inset = [0.0f64, 0.0f64];
    for _ in 0..4 {
        let plot_h = (frame.bottom() - top - bottom_rows * lh - LABEL_GAP - x_title).max(MIN_PLOT * 0.5);
        let y = axis::value_axis_room(
            &ValueSpec {
                data: y_extent,
                zero,
                min: block.y.min,
                max: block.y.max,
                length: plot_h,
                horizontal: false,
                format: &y_format,
                font: &ctx.style.figures,
                size,
            },
            if kind == ChartType::Scatter {
                point_room(ctx)
            } else {
                0.0
            },
        );
        let names_left = if end_names && kind == ChartType::Area {
            place::area_name_room(ctx)
        } else {
            0.0
        };
        let left = frame.x
            + (y.widest(&ctx.style.figures, size) + LABEL_GAP)
                .max(inset[0])
                .max(names_left);
        // A series marker on the last x keeps its whole disk inside the chart.
        let edge = if ctx.series.iter().any(|s| s.shape.is_some()) {
            MARKER_RADIUS + MARKER_STROKE / 2.0 + 0.5
        } else {
            2.0
        };
        let mut right_pad = if end_names && kind != ChartType::Area {
            end_gutter.max(inset[1])
        } else {
            inset[1].max(edge)
        };
        if matches!(kind, ChartType::Line | ChartType::Area) {
            // The nearest-x hover dot reaches 6px plus its 2px halo. Leave one more
            // CSS pixel for backing-canvas rounding at supported device scales.
            right_pad = right_pad.max(edge).max(9.0);
        }
        let plot = Rect::new(left, top, (frame.right() - right_pad - left).max(40.0), plot_h);
        // The x axis reads only the plot's left edge and width, so a pass that changes only the
        // rows under it keeps the last pass's axis (PRF-19).
        let xl = match layout.take() {
            Some((last, xl)) if (last.plot.x, last.plot.w) == (plot.x, plot.w) => xl,
            _ => x_layout(ctx, p, plot, scatter_extent),
        };
        let rows_needed = xl.ticks.iter().map(|(_, l)| l.len()).max().unwrap_or(1).max(1) as f64;
        // x labels are centred on their ticks: the first keeps inside the chart.
        let overhang = |x: f64, lines: &Vec<String>| font.lines_width(lines, size) / 2.0 - (x - frame.x);
        let left_over = xl.ticks.iter().map(|(x, l)| overhang(*x, l)).fold(0.0, f64::max);
        // The last label leans in from its tick at paint time, so it never narrows the plot.
        let settled = rows_needed == bottom_rows && left_over <= 0.5;
        if left_over > 0.5 {
            inset[0] = (left - frame.x) + left_over;
        }
        bottom_rows = rows_needed;
        layout = Some((Frame { plot, y }, xl));
        if settled {
            break;
        }
    }
    let (fr, xl) = layout.expect("laid out");
    PanelLayout {
        fr,
        xl,
        x_title_lines,
        bottom_rows,
        top,
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_panel(
    ctx: &mut Ctx,
    read: &Read,
    p: &Parsed,
    members: &Members,
    frame: Rect,
    title_room: f64,
    y_extent: [f64; 2],
    end_names: bool,
    horizontal: bool,
    facet: usize,
) -> Panel {
    if horizontal {
        return paint_horizontal_bars(ctx, p, members, frame, title_room, y_extent, facet);
    }
    let block = ctx.block;
    let kind = block.kind;
    let size = ctx.size();
    let lh = ctx.line_height();
    let font = ctx.style.font.clone();
    // Lines and points read prepared indices directly. Only annotations need all
    // references here; lines also retain the first row for their derived-record key.
    let count = if block.annotations.is_empty() {
        match kind {
            ChartType::Line | ChartType::Area => 1,
            ChartType::Scatter => 0,
            _ => members.obs.len(),
        }
    } else {
        members.obs.len()
    };
    let obs: Vec<&Obs> = members.obs.iter().take(count).map(|&k| &p.obs[k]).collect();

    // Bubbles and labels keep room within the data's sign (FEA-CH-19).
    let y_extent = if kind == ChartType::Scatter {
        pad_extent(y_extent, 0.04)
    } else {
        y_extent
    };
    let PanelLayout {
        fr,
        xl,
        x_title_lines,
        bottom_rows,
        top,
    } = panel_layout(ctx, p, members, frame, title_room, y_extent, end_names);
    let plot = fr.plot;
    let ymap = |v: f64| fr.y.map(v, plot.bottom(), plot.y);
    let mut items: Vec<Item> = Vec::new();
    let ink = ctx.style.ink;

    // Gridlines and value labels.
    for (t, label) in fr.y.ticks.iter().zip(&fr.y.labels) {
        let y = ymap(*t);
        let is_zero = *t == 0.0 && fr.y.lo <= 0.0 && fr.y.hi >= 0.0;
        if !is_zero {
            items.push(Item::Line {
                path: Path::polyline(&[[plot.x, y], [plot.right(), y]]),
                stroke: Stroke::solid(Style::rgba(ctx.style.rule_soft), GRID_WIDTH),
                role: Role::Grid,
                series: None,
                obs: None,
            });
        }
        items.push(Item::Text(ctx.text_mid(
            label,
            plot.x - LABEL_GAP,
            y,
            Anchor::End,
            Face::Figures,
            Role::AxisLabel,
            ink,
        )));
    }
    // An axis that fits its data ends between ticks: a rule at the foot keeps the plot anchored.
    if fr.y.ticks.first().is_some_and(|t| *t > fr.y.lo + 1e-9) {
        items.push(Item::Line {
            path: Path::polyline(&[[plot.x, plot.bottom()], [plot.right(), plot.bottom()]]),
            stroke: Stroke::solid(Style::rgba(ctx.style.rule_soft), GRID_WIDTH),
            role: Role::Grid,
            series: None,
            obs: None,
        });
    }
    if let Some(label) = &block.y.label {
        items.push(Item::Text(ctx.text(
            label,
            frame.x,
            top - lh / 2.0 - 6.0,
            Anchor::Start,
            Face::Data,
            Role::AxisTitle,
            ink,
        )));
    }
    // x labels.
    let label_top = plot.bottom() + LABEL_GAP;
    let (ascent, _) = font.vertical();
    // The previous label's horizontal extent, so a label that leans in never lands on it.
    let mut previous_right = f64::NEG_INFINITY;
    for (x, lines) in &xl.ticks {
        // A label centred on its tick leans inward when it would pass the figure's edge.
        let half = font.lines_width(lines, size) / 2.0;
        let at = x.clamp(frame.x + half, (frame.right() - half).max(frame.x + half));
        if (at - x).abs() > 0.5 && at - half < previous_right + 4.0 {
            for line in lines {
                let bounds = Rect::new(at - half, label_top, half * 2.0, lh);
                ctx.drop_label(line, DropReason::Collision, "tick", Some(bounds));
            }
            continue;
        }
        previous_right = at + half;
        for (k, line) in lines.iter().enumerate().filter(|(_, line)| !line.is_empty()) {
            let face = if matches!(p.x, XModel::Category { .. }) {
                Face::Data
            } else {
                Face::Figures
            };
            items.push(Item::Text(ctx.text(
                line,
                at,
                label_top + ascent * size + k as f64 * lh,
                Anchor::Middle,
                face,
                Role::AxisLabel,
                ink,
            )));
        }
        if !matches!(p.x, XModel::Category { .. }) {
            items.push(Item::Line {
                path: Path::polyline(&[[*x, plot.bottom()], [*x, plot.bottom() + 4.0]]),
                stroke: Stroke::solid(Style::rgba(ctx.style.ink_faint), 1.0),
                role: Role::Tick,
                series: None,
                obs: None,
            });
        }
    }
    for (k, line) in x_title_lines.iter().enumerate() {
        let y = label_top + bottom_rows * lh + ascent * size + 2.0 + k as f64 * lh;
        // Right-aligned to the plot unless that would start left of the chart.
        let (x, anchor) = if ctx.text_width(line) > plot.right() - frame.x {
            (frame.x, Anchor::Start)
        } else {
            (plot.right(), Anchor::End)
        };
        items.push(Item::Text(ctx.text(
            line,
            x,
            y,
            anchor,
            Face::Data,
            Role::AxisTitle,
            ink,
        )));
    }
    if !xl.unfit.is_empty() {
        let names = xl.unfit.clone();
        ctx.check(
            "label-unfit",
            Level::Warning,
            format!(
                "{} category names do not fit their space: {}",
                names.len(),
                names.join(", ")
            ),
            Some("Shorten the names, or set style.orientation: horizontal."),
            Some("chart.encoding.x".into()),
            Some(Rect::new(plot.x, label_top, plot.w, bottom_rows * lh)),
            json!({"names": names}),
        );
    }
    if let Some(thinned) = &xl.thinned {
        ctx.check(
            "label-thinned",
            Level::Info,
            format!("Category labels show {thinned}"),
            None,
            Some("chart.encoding.x".into()),
            None,
            Value::Null,
        );
    }

    let mut observations = Vec::new();
    let mut labels_taken: Vec<Rect> = items
        .iter()
        .filter_map(|i| if let Item::Text(t) = i { Some(t.bounds) } else { None })
        .collect();
    match kind {
        ChartType::Line | ChartType::Area => {
            if kind == ChartType::Line
                && !block.stack
                && block.style.points.is_none()
                && read.points > crate::limits::max(kind.name(), "all-points observations")
            {
                observations = ctx.observations.get_mut(facet).map(std::mem::take).unwrap_or_default();
            }
            paint_lines(
                ctx,
                read,
                p,
                members,
                &obs,
                plot,
                &xl,
                &ymap,
                &fr.y,
                &mut items,
                &mut observations,
                end_names,
                &mut labels_taken,
            );
        }
        ChartType::Bar => paint_bars(ctx, p, &obs, plot, &xl, &ymap, &fr.y, &mut items, &mut observations),
        ChartType::Scatter => paint_points(
            ctx,
            read,
            p,
            members,
            plot,
            &xl,
            &ymap,
            &mut items,
            &mut observations,
            &mut labels_taken,
        ),
        _ => {}
    }
    // Zero line on top of fills, under labels (STY-40).
    if fr.y.lo <= 0.0 && fr.y.hi >= 0.0 {
        let y = ymap(0.0);
        items.push(Item::Line {
            path: Path::polyline(&[[plot.x, y], [plot.right(), y]]),
            stroke: Stroke::solid(Style::rgba(ink), BASELINE_WIDTH),
            role: Role::Baseline,
            series: None,
            obs: None,
        });
    }
    // An unannotated zoom needs only its visible observations. Mapping the whole source
    // for collision avoidance would do work proportional to the data on every frame.
    if !ctx.block.annotations.is_empty() {
        super::place::annotations(
            ctx,
            plot,
            &xl_map(&xl, &p.x),
            &ymap,
            &mut items,
            &mut labels_taken,
            &obs_points(p, &obs, &xl, &ymap),
        );
    }
    let targeting = match kind {
        ChartType::Line | ChartType::Area => Targeting::NearestX,
        ChartType::Scatter => Targeting::Radius,
        _ => Targeting::Contain,
    };
    let mut panel = Panel {
        facet: None,
        frame,
        plot,
        items,
        observations,
        targeting,
        x: xl.scale.clone(),
        failure: None,
    };
    match kind {
        ChartType::Bar => place::value_labels(
            ctx,
            facet,
            &mut panel,
            place::ValueLabels::Bars {
                horizontal: false,
                stacked: block.stack,
            },
        ),
        ChartType::Line | ChartType::Area if !block.stack => {
            place::value_labels(ctx, facet, &mut panel, place::ValueLabels::Series)
        }
        _ => {}
    }
    panel
}

/// Map an annotation anchor's x (data value or category name) to pixels.
fn xl_map<'a>(xl: &'a XLayout, model: &'a XModel) -> impl Fn(&Value) -> Option<f64> + 'a {
    move |v: &Value| match model {
        XModel::Calendar { .. } => v.as_str().and_then(time::parse_iso).map(|(ms, _)| xl.map(ms as f64)),
        XModel::Numeric { .. } => match v {
            Value::Number(n) => n.as_f64().map(|n| xl.map(n)),
            Value::String(s) => rows::parse_decimal(s).and_then(Result::ok).map(|n| xl.map(n)),
            _ => None,
        },
        XModel::Category { names, .. } => {
            let key = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            names.iter().position(|n| *n == key).map(|i| xl.map(i as f64))
        }
    }
}

/// Every drawn observation's anchor, so annotation text keeps clear of marks.
fn obs_points(p: &Parsed, obs: &[&Obs], xl: &XLayout, ymap: &impl Fn(f64) -> f64) -> Vec<[f64; 2]> {
    let _ = p;
    obs.iter().filter_map(|o| o.y.map(|y| [xl.map(o.x), ymap(y)])).collect()
}

/// Pixels an axis that fits its data keeps clear beyond the first and last point: the radius of the
/// largest mark and a gap, so no disk crosses the axis line.
fn point_room(ctx: &Ctx) -> f64 {
    let radius = if ctx.block.encoding.size.is_some() {
        BUBBLE_MAX
    } else {
        POINT_RADIUS
    };
    radius + 2.0
}

fn pad_extent([lo, hi]: [f64; 2], by: f64) -> [f64; 2] {
    let span = (hi - lo).abs().max(hi.abs().max(1.0) * 0.1);
    // Padding never leaves the representable range, even around ±1e308.
    let mut a = (lo - span * by).max(f64::MIN);
    let mut b = (hi + span * by).min(f64::MAX);
    // Padding stays within the data's sign.
    if lo >= 0.0 && a < 0.0 {
        a = 0.0;
    }
    if hi <= 0.0 && b > 0.0 {
        b = 0.0;
    }
    [a, b]
}

fn heading_for_x(ctx: &Ctx, p: &Parsed, x: f64) -> String {
    match &p.x {
        XModel::Calendar { precision, .. } => axis::full_date(x as i64, *precision, ctx.calendar_locale()),
        XModel::Numeric { years, .. } => {
            if *years {
                format!("{}", x.round())
            } else {
                let name = ctx
                    .block
                    .x
                    .label
                    .clone()
                    .or_else(|| ctx.block.encoding.x.as_deref().map(readable_name))
                    .unwrap_or_default();
                let f = ctx.block.readout_format(&ctx.block.x, ctx.locale);
                format!("{name} {}", numbers::value(x, &f))
            }
        }
        XModel::Category { names, .. } => names.get(x as usize).cloned().unwrap_or_default(),
    }
}

fn series_label(ctx: &Ctx, p: &Parsed, s: usize) -> String {
    if ctx.block.encoding.color.is_some() && !p.self_colour {
        ctx.series[s].name.clone()
    } else {
        ctx.block
            .y
            .label
            .clone()
            .or_else(|| ctx.block.encoding.y.as_deref().map(readable_name))
            .unwrap_or_default()
    }
}

/// Source points retained by a line's zoom, including the neighbours that continue its edge.
fn line_span(p: &Parsed, pts: &[usize], window: Option<[f64; 2]>) -> std::ops::Range<usize> {
    let x_of = |k: usize| p.obs[k].x;
    window.map_or(0..pts.len(), |w| {
        let first = pts.partition_point(|&k| x_of(k) < w[0]);
        let end = pts.partition_point(|&k| x_of(k) <= w[1]);
        let start = match first.checked_sub(1) {
            Some(last) if first < end => {
                let before = x_of(pts[last]);
                pts[..first].partition_point(|&k| x_of(k) < before)
            }
            _ => first,
        };
        let end = match pts.get(end) {
            Some(&k) => {
                let after = x_of(k);
                end + pts[end..].partition_point(|&j| x_of(j) <= after)
            }
            None => end,
        };
        start..end
    })
}

/// First area boundaries in data units, shared by name feasibility and painting.
fn area_name_values(
    ctx: &Ctx,
    read: &Read,
    p: &Parsed,
    members: &Members,
    window: Option<[f64; 2]>,
    all_shown: bool,
) -> Vec<(usize, f64, f64)> {
    let format = ctx.block.readout_format(&ctx.block.y, ctx.locale);
    let spans: Vec<_> = members.series.iter().map(|pts| line_span(p, pts, window)).collect();
    let calendar_gap = match &p.x {
        XModel::Calendar { spacing, .. } => Some(*spacing as f64 * 1.5),
        _ => None,
    };
    let gap = |pts: &[usize], at: usize, start: usize| {
        at > start && calendar_gap.is_some_and(|d| p.obs[pts[at]].x - p.obs[pts[at - 1]].x > d)
    };
    let key = |x: f64| (x * 1000.0).round() as i64;
    let mut out = Vec::new();
    for (s, pts) in members.series.iter().enumerate() {
        let hidden = !all_shown && ctx.is_hidden(s);
        let visible =
            |&k: &usize| p.obs[k].y.is_some() && window.is_none_or(|w| p.obs[k].x >= w[0] && p.obs[k].x <= w[1]);
        let valid = |&k: &usize| p.obs[k].y.is_some();
        let selected = if hidden {
            pts.iter().position(visible).or_else(|| pts.iter().position(valid))
        } else {
            let points = &pts[spans[s].clone()];
            points
                .iter()
                .position(visible)
                .or_else(|| points.iter().position(valid))
                .map(|at| spans[s].start + at)
        };
        let Some(at) = selected else { continue };
        let k = pts[at];
        let value = p.obs[k].y.expect("selected a valid point");
        let (mut top, mut shown) = (value, read.shown(k, value, &format));
        if ctx.block.stack && !hidden && !gap(pts, at, spans[s].start) {
            top = 0.0;
            shown = 0.0;
            let wanted = key(p.obs[k].x);
            for (other, (points, span)) in members.series.iter().zip(&spans).enumerate().take(s + 1) {
                if !all_shown && ctx.is_hidden(other) {
                    continue;
                }
                let span = span.clone();
                let lo = span.start + points[span.clone()].partition_point(|&k| key(p.obs[k].x) < wanted);
                let mut hi = span.start + points[span.clone()].partition_point(|&k| key(p.obs[k].x) <= wanted);
                if other == s {
                    hi = hi.min(at + 1);
                }
                for j in lo..hi {
                    // A calendar gap starts a new run without adding to the stack at that x.
                    if gap(points, j, span.start) {
                        continue;
                    }
                    let k = points[j];
                    if let Some(v) = p.obs[k].y
                        && (v >= 0.0) == (value >= 0.0)
                    {
                        top += v;
                        shown += read.shown(k, v, &format);
                    }
                }
            }
            shown = numbers::shown(shown, &format);
        }
        out.push((s, top, shown));
    }
    out
}

/// A name cannot drift far enough to appear to identify the next value tick.
fn area_name_limit(y: &ValueAxis, plot: Rect) -> f64 {
    y.ticks
        .windows(2)
        .map(|t| (y.map(t[1], plot.bottom(), plot.y) - y.map(t[0], plot.bottom(), plot.y)).abs())
        .reduce(f64::min)
        .unwrap_or(plot.h)
        / 2.0
}

/// The narrowest stroke of a dense series' line, in CSS pixels.
const DENSE_LINE_WIDTH: f64 = 1.25;

/// The stroke of a line with `per_px` observations per pixel of plot width: the full
/// `LINE_WIDTH` while observations stand four or more pixels apart, thinning evenly to
/// `DENSE_LINE_WIDTH` at one per pixel and beyond, so a dense series reads as a line and not
/// as a ribbon. Zooming in spreads the observations and the line regains its weight.
fn line_width(per_px: f64) -> f64 {
    let t = ((per_px - 0.25) / 0.75).clamp(0.0, 1.0);
    LINE_WIDTH - t * (LINE_WIDTH - DENSE_LINE_WIDTH)
}

#[allow(clippy::too_many_arguments)]
fn paint_lines(
    ctx: &mut Ctx,
    read: &Read,
    p: &Parsed,
    members: &Members,
    obs: &[&Obs],
    plot: Rect,
    xl: &XLayout,
    ymap: &impl Fn(f64) -> f64,
    y: &ValueAxis,
    items: &mut Vec<Item>,
    observations: &mut Vec<Observation>,
    end_names: bool,
    taken: &mut Vec<Rect>,
) {
    let block = ctx.block;
    let area = block.kind == ChartType::Area;
    let automatic =
        block.style.points.is_none() && read.points > crate::limits::max(block.kind.name(), "all-points observations");
    let points_mode = if automatic {
        crate::block::Points::M4
    } else {
        block.style.points.unwrap_or(crate::block::Points::All)
    };
    let curve_kind = block.style.curve.unwrap_or(ctx.style.curve);
    let strength = block.style.curvature.unwrap_or(ctx.style.curvature);
    let readout = block.readout_format(&block.y, ctx.locale);
    let n_series = ctx.series.len();
    let calendar_gap = match &p.x {
        XModel::Calendar { spacing, .. } => Some(*spacing as f64 * 1.5),
        _ => None,
    };
    let window = xl.scale.as_ref().map(|s| s.domain);
    let baseline = ymap(0.0f64.clamp(y.lo, y.hi));
    // Stack bases per x, for stacked areas (FEA-CH-05).
    let mut stack_pos: std::collections::HashMap<i64, f64> = Default::default();
    let mut stack_neg: std::collections::HashMap<i64, f64> = Default::default();
    let mut shown_stack_pos: std::collections::HashMap<i64, f64> = Default::default();
    let mut shown_stack_neg: std::collections::HashMap<i64, f64> = Default::default();
    let mut ends: Vec<(usize, f64, f64)> = Vec::new();
    let mut dense_note = None;
    let key = |x: f64| (x * 1000.0).round() as i64;
    let painted_from = items.len();
    for s in 0..n_series {
        let pts = &members.series[s];
        if ctx.is_hidden(s) {
            // A hidden series keeps its name at its line's end, dimmed, so it can come back
            // (FEA-CH-27, FEA-CH-33).
            if let Some(&last) = pts.iter().rev().find(|&&k| {
                let o = &p.obs[k];
                o.y.is_some() && window.is_none_or(|w| o.x >= w[0] && o.x <= w[1])
            }) {
                let value = p.obs[last].y.unwrap_or(0.0);
                ends.push((s, ymap(value.clamp(y.lo, y.hi)), read.shown(last, value, &readout)));
            }
            continue;
        }
        // Outside a zoom window, a line keeps the nearest x on each side, so it continues to the
        // plot edge: the last x before the window when the window holds a point, and the first x
        // after it. The points are in x order, so bisection finds the window and both
        // neighbours, and a zoomed layout reads only the points it draws (S27-837, PRF-19).
        let span = line_span(p, pts, window);
        // Runs split at missing values and calendar gaps (FEA-CH-38).
        let mut runs: Vec<Vec<(f64, f64, f64)>> = vec![Vec::with_capacity(span.len())];
        let mut previous_x: Option<f64> = None;
        let mut last_shown_value = None;
        for &k in &pts[span] {
            let o = &p.obs[k];
            let gap = matches!((previous_x, calendar_gap), (Some(px), Some(g)) if o.x - px > g);
            previous_x = Some(o.x);
            match o.y {
                Some(v) if !gap => {
                    let shown = read.shown(k, v, &readout);
                    let shown_top = if block.stack {
                        let base = if v >= 0.0 {
                            shown_stack_pos.entry(key(o.x)).or_default()
                        } else {
                            shown_stack_neg.entry(key(o.x)).or_default()
                        };
                        *base += shown;
                        numbers::shown(*base, &readout)
                    } else {
                        shown
                    };
                    last_shown_value = Some(shown_top);
                    let (lo, hi) = if block.stack {
                        let base = if v >= 0.0 {
                            stack_pos.entry(key(o.x)).or_default()
                        } else {
                            stack_neg.entry(key(o.x)).or_default()
                        };
                        let lo = *base;
                        *base += v;
                        (lo, *base)
                    } else {
                        (0.0, v)
                    };
                    runs.last_mut()
                        .expect("run")
                        .push((xl.map(o.x), ymap(hi), ymap(lo.clamp(y.lo, y.hi))));
                }
                Some(v) => {
                    last_shown_value = Some(read.shown(k, v, &readout));
                    runs.push(Vec::new());
                    let (lo, hi) = (0.0, v);
                    runs.last_mut().expect("run").push((xl.map(o.x), ymap(hi), ymap(lo)));
                }
                None if block.style.bridge_gaps => {}
                None => runs.push(Vec::new()),
            }
        }
        runs.retain(|r| !r.is_empty());
        let color = Style::rgba(ctx.series_color(s));
        let per_px = runs.iter().map(Vec::len).sum::<usize>() as f64 / plot.w.max(1.0);
        let mut stroke = Stroke::solid(color, line_width(per_px));
        stroke.cap = Cap::Butt;
        stroke.dash = ctx.series[s].dash.clone();
        // Narrow breaks get a rug mark (FEA-CH-38).
        for pair in runs.windows(2) {
            let (a, b) = (pair[0].last().expect("run").0, pair[1][0].0);
            if b - a < RUG_BELOW {
                items.push(Item::Rect {
                    rect: Rect::new(a, plot.bottom() - 4.0, (b - a).max(1.0), 4.0),
                    fill: Fill::solid(Style::rgba(ctx.style.ink_soft)),
                    stroke: None,
                    radius: [0.0; 4],
                    role: Role::Rug,
                    series: Some(s),
                    obs: None,
                });
            }
        }
        let (mut total, mut kept) = (0, 0);
        for run in &runs {
            total += run.len();
            let pixels = plot.w.max(1.0);
            let density = run.len() as f64 / pixels;
            let points: Vec<[f64; 2]> = run.iter().map(|r| [r.0, r.1]).collect();
            if points_mode == crate::block::Points::Band && density > 6.0 && !block.stack {
                // Mean line and range band per pixel column (FEA-CH-17).
                let values: Vec<f64> = run.iter().map(|r| r.1).collect();
                let px: Vec<f64> = run.iter().map(|r| r.0).collect();
                let cols = curve::columns(&px, &values);
                kept += cols.len();
                let mut band = Path::new();
                let upper: Vec<[f64; 2]> = cols.iter().map(|c| [c.x, c.min]).collect();
                let lower: Vec<[f64; 2]> = cols.iter().rev().map(|c| [c.x, c.max]).collect();
                curve::append_run(&mut band, &upper, crate::block::Curve::Linear, 0.0, false);
                curve::append_run(&mut band, &lower, crate::block::Curve::Linear, 0.0, true);
                band.close();
                items.push(Item::Area {
                    path: band,
                    fill: Fill::solid(color.with_alpha(BAND_OPACITY)),
                    role: Role::Band,
                    series: Some(s),
                    obs: None,
                });
                let mean: Vec<[f64; 2]> = cols.iter().map(|c| [c.x, c.mean]).collect();
                let mut path = Path::new();
                curve::append_run(&mut path, &mean, crate::block::Curve::Linear, 0.0, false);
                items.push(Item::Line {
                    path,
                    stroke: stroke.clone(),
                    role: Role::Mark,
                    series: Some(s),
                    obs: None,
                });
                dense_note = Some(density.round());
                continue;
            }
            let reduced = if points_mode != crate::block::Points::All && density > 1.0 {
                curve::m4(&points)
            } else {
                points
            };
            kept += reduced.len();
            if area {
                let mut fill = Path::new();
                if block.stack {
                    let lower: Vec<[f64; 2]> = run.iter().rev().map(|r| [r.0, r.2]).collect();
                    curve::append_run(&mut fill, &reduced, crate::block::Curve::Linear, 0.0, false);
                    curve::append_run(&mut fill, &lower, crate::block::Curve::Linear, 0.0, true);
                } else {
                    curve::append_run(&mut fill, &reduced, curve_kind, strength, false);
                    let (first, last) = (reduced[0][0], reduced[reduced.len() - 1][0]);
                    fill.line_to([last, baseline]);
                    fill.line_to([first, baseline]);
                }
                fill.close();
                let hatch = ctx.series[s].hatch;
                items.push(Item::Area {
                    path: fill,
                    fill: Fill {
                        color: color.with_alpha(AREA_OPACITY),
                        hatch,
                        hatch_ink: hatch.map(|_| color),
                        hatch_scale: 1.0,
                    },
                    role: Role::Mark,
                    series: Some(s),
                    obs: None,
                });
            }
            if reduced.len() == 1 {
                // An isolated observation between breaks still shows.
                items.push(Item::Point {
                    center: reduced[0],
                    radius: LINE_WIDTH,
                    shape: Shape::Circle,
                    fill: Some(Fill::solid(color)),
                    stroke: None,
                    role: Role::Mark,
                    series: Some(s),
                    obs: None,
                });
            } else {
                let mut path = Path::new();
                let (c, k) = if block.stack {
                    (crate::block::Curve::Linear, 0.0)
                } else {
                    (curve_kind, strength)
                };
                curve::append_run(&mut path, &reduced, c, k, false);
                items.push(Item::Line {
                    path,
                    stroke: stroke.clone(),
                    role: Role::Mark,
                    series: Some(s),
                    obs: None,
                });
            }
            if let Some(shape) = ctx.series[s].shape {
                let step = (reduced.len() / 10).max(1);
                for pt in reduced.iter().step_by(step) {
                    items.push(Item::Point {
                        center: *pt,
                        radius: MARKER_RADIUS,
                        shape,
                        fill: Some(Fill::solid(Style::rgba(ctx.style.surface))),
                        stroke: Some(Stroke::solid(color, MARKER_STROKE)),
                        role: Role::Mark,
                        series: Some(s),
                        obs: None,
                    });
                }
            }
        }
        if kept < total || automatic {
            ctx.check(
                "decimation",
                Level::Info,
                if automatic {
                    format!("{} uses M4 above the all-points ceiling: {kept} positions from {total} observations in this view", ctx.series[s].name)
                } else {
                    format!("{} draws {kept} representative positions from {total} observations", ctx.series[s].name)
                },
                None,
                Some("chart.style.points".into()),
                Some(plot),
                json!({"perPixel": total as f64 / plot.w.max(1.0), "kept": kept, "total": total,
                    "series": ctx.series[s].key, "automatic": automatic,
                    "ceiling": crate::limits::max(block.kind.name(), "all-points observations")}),
            );
        }
        if let Some(last) = runs.iter().rev().flat_map(|r| r.iter().rev()).next() {
            ends.push((s, last.1, last_shown_value.unwrap_or(0.0)));
        }
    }
    // Every translucent fill goes under every line, so a later series never veils an earlier
    // series' line and overlaps blend in the series' own hues.
    items[painted_from..].sort_by_key(|i| !matches!(i, Item::Area { role: Role::Mark, .. }));
    if let Some(d) = dense_note {
        ctx.note.push(format!(
            "≈{d} observations per pixel: lines show the mean, bands the range"
        ));
        ctx.check(
            "aggregation",
            Level::Info,
            format!("About {d} observations per pixel; lines show per-pixel means and bands the range"),
            None,
            None,
            None,
            json!({"perPixel": d}),
        );
    }
    if area {
        ends = area_name_values(ctx, read, p, members, window, false)
            .into_iter()
            .map(|(s, value, shown)| (s, ymap(value.clamp(y.lo, y.hi)), shown))
            .collect();
    }
    // Data records do not depend on whether the style has room for direct names.
    let end_order = place::end_name_order(ctx, panel_of(obs), &ends);
    if end_names {
        place::end_names(
            ctx,
            plot,
            &ends,
            &end_order,
            items,
            taken,
            if area { Some(area_name_limit(y, plot)) } else { None },
        );
    }
    // One readout per distinct x listing every visible series in legend order (FEA-CH-41).
    // The totals of a stack are sums that jump with the data, so the exact rows write them
    // (TYP-36).
    let totals: Vec<String> = if block.stack {
        let key = format!("cartesian.totals.{:?}", ctx.opts.hidden);
        ctx.derive(panel_of(obs), &key, || {
            members
                .at_x
                .iter()
                .map(|a| numbers::value(a.total(ctx), &readout))
                .collect()
        })
    } else {
        Vec::new()
    };
    // The distinct x values are in x order, so bisection finds the window's (PRF-19).
    let at = &members.at_x;
    let shown = window.map_or(0..at.len(), |w| {
        at.partition_point(|a| a.x() < w[0] - 1e-9)..at.partition_point(|a| a.x() <= w[1] + 1e-9)
    });
    if ctx.series.iter().any(|s| !s.hidden) {
        observations.reserve(shown.len().saturating_sub(observations.len()));
    }
    let recycled = !observations.is_empty();
    let mut emitted = 0;
    // Where the observations of each series' labelled values land in this window.
    let wanted: Vec<usize> = members.kinds.iter().flatten().flatten().copied().collect();
    let mut landed: Vec<(usize, usize)> = Vec::new();
    for (at_x, a) in at.iter().enumerate().take(shown.end).skip(shown.start) {
        let x = a.x();
        let px = xl.map(x);
        let mut dots = observations
            .get_mut(emitted)
            .map(|o| std::mem::take(&mut o.dots))
            .unwrap_or_default();
        dots.clear();
        if !block.stack {
            let mut values = a.values(ctx).map(|(s, v)| Dot {
                at: [px, ymap(v)],
                series: s,
            });
            if n_series == 1 {
                // A filtered iterator otherwise grows its first allocation to four dots.
                // The single-series readout can contain only this one.
                if let Some(dot) = values.next() {
                    dots.reserve_exact(1);
                    dots.push(dot);
                }
            } else {
                dots.extend(values);
            }
        }
        let mut shown = Rc::clone(a.readout(ctx, p, &readout));
        if block.stack {
            Rc::make_mut(&mut shown).lines.push(ReadoutLine {
                name: "Total".into(),
                value: totals
                    .get(at_x)
                    .cloned()
                    .unwrap_or_else(|| numbers::value(a.total(ctx), &readout)),
                series: None,
                names_series: false,
                derived: true,
            });
        }
        if shown.lines.is_empty() {
            continue;
        }
        let at = dots.first().map_or([px, plot.y + plot.h / 2.0], |d| d.at);
        let observation = Observation {
            row: None,
            dots,
            series: None,
            at,
            wedge: None,
            area: None,
            radius: 0.0,
            x: px,
            readout: shown,
        };
        if let Some(slot) = observations.get_mut(emitted) {
            *slot = observation;
        } else {
            observations.push(observation);
        }
        if wanted.contains(&at_x) {
            landed.push((at_x, emitted));
        }
        emitted += 1;
    }
    observations.truncate(emitted);
    ctx.value_kinds = members
        .kinds
        .iter()
        .map(|kinds| kinds.map(|k| k.and_then(|i| landed.iter().find(|(a, _)| *a == i).map(|(_, o)| *o))))
        .collect();
    // A narrow zoom need not retain the full view's observation allocation.
    if recycled && observations.capacity() > observations.len().saturating_mul(4).max(16) {
        observations.shrink_to_fit();
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_bars(
    ctx: &mut Ctx,
    p: &Parsed,
    obs: &[&Obs],
    _plot: Rect,
    xl: &XLayout,
    ymap: &impl Fn(f64) -> f64,
    y: &ValueAxis,
    items: &mut Vec<Item>,
    observations: &mut Vec<Observation>,
) {
    let block = ctx.block;
    let XModel::Category { names, .. } = &p.x else { return };
    let readout = block.readout_format(&block.y, ctx.locale);
    let groups = if block.stack || p.self_colour {
        1
    } else {
        ctx.series.iter().filter(|s| !s.hidden).count().max(1)
    };
    let visible: Vec<usize> = (0..ctx.series.len()).filter(|s| !ctx.is_hidden(*s)).collect();
    let inner = xl.band * 0.72;
    let thickness = (inner / groups as f64).min(64.0);
    let zero_y = ymap(0.0f64.clamp(y.lo, y.hi));
    let clearance = BASELINE_WIDTH;
    let radius_cap = |t: f64| ctx.style.bar_radius.min(t * 0.15);
    let mut stack_pos = vec![0.0f64; names.len()];
    let mut stack_neg = vec![0.0f64; names.len()];
    for (cat, name) in names.iter().enumerate() {
        let center = xl.map(cat as f64);
        for (slot, &s) in visible.iter().enumerate() {
            if p.self_colour && s != 0 {
                continue;
            }
            let series_index = if p.self_colour { cat } else { s };
            if p.self_colour && ctx.is_hidden(cat) {
                continue;
            }
            let o = obs
                .iter()
                .find(|o| o.x as usize == cat && (p.self_colour || o.series == s));
            let left = if groups == 1 {
                center - thickness / 2.0
            } else {
                center - inner / 2.0 + slot as f64 * thickness
            };
            let Some(o) = o else {
                // Impossible combinations reserve no slot when bars colour themselves.
                continue;
            };
            let color = Style::rgba(if p.self_colour {
                ctx.style.palette[ctx.series.get(cat).map_or(cat, |s| s.slot) % ctx.style.palette.len()]
            } else {
                ctx.series_color(s)
            });
            match o.y {
                None => {
                    let t = ctx.text(
                        "n/a",
                        left + thickness / 2.0,
                        zero_y - 4.0,
                        Anchor::Middle,
                        Face::Data,
                        Role::Missing,
                        ctx.style.ink_soft,
                    );
                    items.push(Item::Text(t));
                    observations.push(bar_observation(
                        ctx,
                        p,
                        o,
                        Rect::new(left, zero_y - 14.0, thickness, 14.0),
                        name,
                        None,
                        &readout,
                    ));
                }
                Some(v) => {
                    let (from, to) = if block.stack {
                        let base = if v >= 0.0 {
                            &mut stack_pos[cat]
                        } else {
                            &mut stack_neg[cat]
                        };
                        let from = *base;
                        *base += v;
                        (from, *base)
                    } else {
                        (0.0, v)
                    };
                    let (y0, y1) = (ymap(from), ymap(to));
                    let mut top = y0.min(y1);
                    let mut bottom = y0.max(y1);
                    if from == 0.0 {
                        if v >= 0.0 {
                            bottom -= clearance / 2.0
                        } else {
                            top += clearance / 2.0
                        }
                    }
                    // A value that is not zero keeps a pixel of ink, however small (FEA-CH-03).
                    if v != 0.0 && bottom - top < 1.0 {
                        if v > 0.0 {
                            top = bottom - 1.0;
                        } else {
                            bottom = top + 1.0;
                        }
                    }
                    let rect =
                        Rect::new(left + 0.5, top, (thickness - 1.0).max(1.0), (bottom - top).max(0.0)).clamped();
                    let r = radius_cap(rect.w).min(rect.h);
                    let is_top = !block.stack || stack_top(obs, cat, s, v, block.stack);
                    let radius = if !is_top {
                        [0.0; 4]
                    } else if v >= 0.0 {
                        [r, r, 0.0, 0.0]
                    } else {
                        [0.0, 0.0, r, r]
                    };
                    let hatch = if p.self_colour {
                        ctx.series.get(cat).and_then(|s| s.hatch)
                    } else {
                        ctx.series[s].hatch
                    };
                    if rect.h > 0.0 {
                        items.push(Item::Rect {
                            rect,
                            fill: Fill {
                                color,
                                hatch,
                                hatch_ink: hatch.map(|_| Style::rgba(ctx.style.surface)),
                                hatch_scale: 1.0,
                            },
                            stroke: None,
                            radius,
                            role: Role::Mark,
                            series: Some(series_index),
                            obs: Some(observations.len()),
                        });
                    }
                    // A zero draws no length but stays inspectable (FEA-CH-06).
                    let hit = if rect.h < 6.0 {
                        Rect::new(rect.x, rect.y - 3.0, rect.w, rect.h + 6.0)
                    } else {
                        rect
                    };
                    observations.push(bar_observation(ctx, p, o, hit, name, Some(v), &readout));
                }
            }
        }
    }
    if block.stack {
        // The stack top carries the total in readouts; the exact rows write it (TYP-36).
        let key = format!("cartesian.totals.{:?}", ctx.opts.hidden);
        let totals: Vec<String> = ctx.derive(panel_of(obs), &key, || {
            (0..names.len())
                .map(|cat| {
                    let total: f64 = obs
                        .iter()
                        .filter(|o| o.x as usize == cat && !ctx.is_hidden(o.series))
                        .filter_map(|o| o.y)
                        .sum();
                    numbers::value(total, &readout)
                })
                .collect()
        });
        for (cat, name) in names.iter().enumerate() {
            for ob in observations.iter_mut().filter(|ob| ob.readout.heading == *name) {
                Rc::make_mut(&mut ob.readout).lines.push(ReadoutLine {
                    name: "Total".into(),
                    value: totals.get(cat).cloned().unwrap_or_default(),
                    series: None,
                    names_series: false,
                    derived: true,
                });
            }
        }
    }
}

fn stack_top(obs: &[&Obs], cat: usize, s: usize, v: f64, stack: bool) -> bool {
    if !stack {
        return true;
    }
    // The last visible layer with the same sign holds the rounded end.
    !obs.iter()
        .any(|o| o.x as usize == cat && o.series > s && o.y.is_some_and(|w| (w >= 0.0) == (v >= 0.0)))
}

fn bar_observation(
    ctx: &Ctx,
    p: &Parsed,
    o: &Obs,
    area: Rect,
    category: &str,
    v: Option<f64>,
    f: &Format,
) -> Observation {
    let value = v.map_or_else(|| "No observation".to_owned(), |v| numbers::value(v, f));
    let name = if p.self_colour || ctx.block.encoding.color.is_none() {
        ctx.block
            .y
            .label
            .clone()
            .or_else(|| ctx.block.encoding.y.as_deref().map(readable_name))
            .unwrap_or_default()
    } else {
        ctx.series[o.series].name.clone()
    };
    let series = if p.self_colour { o.x as usize } else { o.series };
    Observation {
        row: Some(o.row),
        dots: Vec::new(),
        series: Some(series),
        at: [
            area.x + area.w / 2.0,
            if v.is_some_and(|value| value < 0.0) {
                area.bottom()
            } else {
                area.y
            },
        ],
        wedge: None,
        area: Some(area),
        radius: 0.0,
        x: area.x + area.w / 2.0,
        readout: Readout {
            heading: category.to_owned(),
            lines: vec![ReadoutLine {
                name,
                value,
                series: Some(series),
                names_series: ctx.block.encoding.color.is_some() && !p.self_colour,
                derived: false,
            }],
        }
        .into(),
    }
}

fn paint_horizontal_bars(
    ctx: &mut Ctx,
    p: &Parsed,
    members: &Members,
    frame: Rect,
    title_room: f64,
    y_extent: [f64; 2],
    facet: usize,
) -> Panel {
    let block = ctx.block;
    let size = ctx.size();
    let lh = ctx.line_height();
    let font = ctx.style.font.clone();
    let XModel::Category { names, reader, .. } = &p.x else {
        unreachable!("bars are categorical")
    };
    let obs: Vec<&Obs> = members.obs.iter().map(|&k| &p.obs[k]).collect();
    let narrow = frame.w < 480.0;
    // Names in a gutter of at most 40% of the width, or above each bar under 480 px.
    let gutter = if narrow {
        0.0
    } else {
        (reader
            .iter()
            .map(|n| font.lines_width(&font.wrap(n, size, frame.w * 0.4, 2), size))
            .fold(0.0, f64::max)
            + LABEL_GAP)
            .min(frame.w * 0.4)
    };
    let top = frame.y + title_room + if block.x.label.is_some() { lh + 4.0 } else { 0.0 } + 2.0;
    let axis_room = lh + LABEL_GAP + if block.y.label.is_some() { lh + 2.0 } else { 0.0 };
    let format = block.label_format(&block.y, ctx.locale);
    let plot_h = (frame.bottom() - top - axis_room).max(MIN_PLOT * 0.5);
    let mut plot = Rect::new(frame.x + gutter, top, frame.w - gutter - 12.0, plot_h);
    let mut x_axis = axis::value_axis(&ValueSpec {
        data: y_extent,
        zero: Zero::Include,
        min: block.y.min,
        max: block.y.max,
        length: plot.w,
        horizontal: true,
        format: &format,
        font: &ctx.style.figures,
        size,
    });
    for _ in 0..2 {
        // Value labels are centred on their ticks: keep the outer ones inside the chart.
        let first = x_axis
            .labels
            .first()
            .map_or(0.0, |l| ctx.style.figures.width(l, size) / 2.0);
        let last = x_axis
            .labels
            .last()
            .map_or(0.0, |l| ctx.style.figures.width(l, size) / 2.0);
        let left = frame.x + gutter.max(first);
        let right = frame.right() - last.max(4.0);
        if (left - plot.x).abs() < 0.5 && (right - plot.right()).abs() < 0.5 {
            break;
        }
        plot = Rect::new(left, top, (right - left).max(40.0), plot_h);
        x_axis = axis::value_axis(&ValueSpec {
            data: y_extent,
            zero: Zero::Include,
            min: block.y.min,
            max: block.y.max,
            length: plot.w,
            horizontal: true,
            format: &format,
            font: &ctx.style.figures,
            size,
        });
    }
    let xmap = |v: f64| x_axis.map(v, plot.x, plot.right());
    let mut items = Vec::new();
    let ink = ctx.style.ink;
    for (t, label) in x_axis.ticks.iter().zip(&x_axis.labels) {
        let x = xmap(*t);
        if *t != 0.0 {
            items.push(Item::Line {
                path: Path::polyline(&[[x, plot.y], [x, plot.bottom()]]),
                stroke: Stroke::solid(Style::rgba(ctx.style.rule_soft), GRID_WIDTH),
                role: Role::Grid,
                series: None,
                obs: None,
            });
        }
        let (ascent, _) = font.vertical();
        items.push(Item::Text(ctx.text(
            label,
            x,
            plot.bottom() + LABEL_GAP + ascent * size,
            Anchor::Middle,
            Face::Figures,
            Role::AxisLabel,
            ink,
        )));
    }
    if let Some(label) = &block.y.label {
        let (ascent, _) = font.vertical();
        items.push(Item::Text(ctx.text(
            label,
            plot.right(),
            plot.bottom() + LABEL_GAP + lh + 2.0 + ascent * size,
            Anchor::End,
            Face::Data,
            Role::AxisTitle,
            ink,
        )));
    }
    if let Some(label) = &block.x.label {
        // On horizontal bars the category title heads the names (FEA-CH-20).
        items.push(Item::Text(ctx.text(
            label,
            frame.x,
            top - 6.0,
            Anchor::Start,
            Face::Data,
            Role::AxisTitle,
            ink,
        )));
    }
    let n = names.len().max(1);
    let band = plot.h / n as f64;
    let groups = if block.stack || p.self_colour {
        1
    } else {
        ctx.series.iter().filter(|s| !s.hidden).count().max(1)
    };
    // Under 480 px each name sits above its bar, wrapped at the plot width (FEA-CH-26).
    let name_inset = if narrow { NAME_INSET } else { 0.0 };
    let name_room = if narrow {
        name_lines(ctx, reader, plot.w - name_inset) as f64 * lh
    } else {
        0.0
    };
    let inner = (band - name_room) * 0.72;
    let thickness = (inner / groups as f64).min(40.0);
    let zero_x = xmap(0.0f64.clamp(x_axis.lo, x_axis.hi));
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut observations = Vec::new();
    let visible: Vec<usize> = (0..ctx.series.len()).filter(|s| !ctx.is_hidden(*s)).collect();
    let mut unfit = Vec::new();
    for (cat, name) in names.iter().enumerate() {
        let band_top = plot.y + cat as f64 * band;
        let center = band_top + name_room + (band - name_room) / 2.0;
        // The name.
        let (room, most) = if narrow {
            (plot.w - name_inset, NAME_LINES_ABOVE)
        } else {
            (gutter - LABEL_GAP, 2)
        };
        let lines = font.wrap(&reader[cat], size, room, most);
        if lines.len() > most || font.lines_width(&lines, size) > room + 0.5 {
            unfit.push(reader[cat].clone());
        }
        for (k, line) in lines.iter().take(most).enumerate() {
            let y = if narrow {
                band_top + font.baseline_offset(size) + k as f64 * lh
            } else {
                center - (lines.len() as f64 - 1.0) * lh / 2.0 + k as f64 * lh
            };
            let t = if narrow {
                ctx.text(
                    line,
                    plot.x + name_inset,
                    y,
                    Anchor::Start,
                    Face::Data,
                    Role::AxisLabel,
                    ink,
                )
            } else {
                ctx.text_mid(
                    line,
                    plot.x - LABEL_GAP,
                    y,
                    Anchor::End,
                    Face::Data,
                    Role::AxisLabel,
                    ink,
                )
            };
            items.push(Item::Text(t));
        }
        let mut stack_pos = 0.0;
        let mut stack_neg = 0.0;
        for (slot, &s) in visible.iter().enumerate() {
            if p.self_colour && s != 0 {
                continue;
            }
            let Some(o) = obs
                .iter()
                .find(|o| o.x as usize == cat && (p.self_colour || o.series == s))
            else {
                continue;
            };
            let series_index = if p.self_colour { cat } else { s };
            let top_y = if groups == 1 {
                center - thickness / 2.0
            } else {
                center - inner / 2.0 + slot as f64 * thickness
            };
            let color = Style::rgba(if p.self_colour {
                ctx.style.palette[ctx.series.get(cat).map_or(cat, |s| s.slot) % ctx.style.palette.len()]
            } else {
                ctx.series_color(s)
            });
            match o.y {
                None => {
                    items.push(Item::Text(ctx.text_mid(
                        "n/a",
                        zero_x + 4.0,
                        top_y + thickness / 2.0,
                        Anchor::Start,
                        Face::Data,
                        Role::Missing,
                        ctx.style.ink_soft,
                    )));
                    observations.push(bar_observation(
                        ctx,
                        p,
                        o,
                        Rect::new(zero_x, top_y, 24.0, thickness),
                        name,
                        None,
                        &readout,
                    ));
                }
                Some(v) => {
                    let (from, to) = if block.stack {
                        let base = if v >= 0.0 { &mut stack_pos } else { &mut stack_neg };
                        let from = *base;
                        *base += v;
                        (from, *base)
                    } else {
                        (0.0, v)
                    };
                    let (x0, x1) = (xmap(from), xmap(to));
                    let mut left = x0.min(x1);
                    let mut right = x0.max(x1);
                    if from == 0.0 {
                        if v >= 0.0 {
                            left += BASELINE_WIDTH / 2.0
                        } else {
                            right -= BASELINE_WIDTH / 2.0
                        }
                    }
                    // A value that is not zero keeps a pixel of ink, however small (FEA-CH-03).
                    if v != 0.0 && right - left < 1.0 {
                        if v > 0.0 {
                            right = left + 1.0;
                        } else {
                            left = right - 1.0;
                        }
                    }
                    let rect = Rect::new(left, top_y + 0.5, (right - left).max(0.0), (thickness - 1.0).max(1.0));
                    let r = ctx.style.bar_radius.min(rect.h * 0.15).min(rect.w);
                    let radius = if v >= 0.0 { [0.0, r, r, 0.0] } else { [r, 0.0, 0.0, r] };
                    let hatch = if p.self_colour {
                        ctx.series.get(cat).and_then(|s| s.hatch)
                    } else {
                        ctx.series[s].hatch
                    };
                    if rect.w > 0.0 {
                        items.push(Item::Rect {
                            rect,
                            fill: Fill {
                                color,
                                hatch,
                                hatch_ink: hatch.map(|_| Style::rgba(ctx.style.surface)),
                                hatch_scale: 1.0,
                            },
                            stroke: None,
                            radius,
                            role: Role::Mark,
                            series: Some(series_index),
                            obs: Some(observations.len()),
                        });
                    }
                    let hit = if rect.w < 6.0 {
                        Rect::new(rect.x - 3.0, rect.y, rect.w + 6.0, rect.h)
                    } else {
                        rect
                    };
                    let mut ob = bar_observation(ctx, p, o, hit, name, Some(v), &readout);
                    ob.at = [if v < 0.0 { hit.x } else { hit.right() }, hit.y + hit.h / 2.0];
                    observations.push(ob);
                }
            }
        }
    }
    if !unfit.is_empty() {
        ctx.check(
            "label-unfit",
            Level::Warning,
            format!(
                "{} category names do not fit beside their bars: {}",
                unfit.len(),
                unfit.join(", ")
            ),
            Some("Shorten the names."),
            Some("chart.encoding.x".into()),
            None,
            json!({"names": unfit}),
        );
    }
    items.push(Item::Line {
        path: Path::polyline(&[[zero_x, plot.y], [zero_x, plot.bottom()]]),
        stroke: Stroke::solid(Style::rgba(ink), BASELINE_WIDTH),
        role: Role::Baseline,
        series: None,
        obs: None,
    });
    let mut panel = Panel {
        facet: None,
        frame,
        plot,
        items,
        observations,
        targeting: Targeting::Contain,
        x: None,
        failure: None,
    };
    place::value_labels(
        ctx,
        facet,
        &mut panel,
        place::ValueLabels::Bars {
            horizontal: true,
            stacked: block.stack,
        },
    );
    break_lines_at_text(&mut panel);
    panel
}

/// Room a vertical line leaves round text it would cross, past the text's box on every side.
const TEXT_GAP: f64 = 2.0;

/// Each vertical gridline or zero line of horizontal bars breaks where it would run through
/// text, `TEXT_GAP` past the text's box, and runs on everywhere else: through the names set
/// above their bars under 480 px, and through value labels set across a gridline at any width.
/// No knockout box is painted, so a tinted plot stays true (FEA-CH-26).
fn break_lines_at_text(panel: &mut Panel) {
    let boxes: Vec<Rect> = panel
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Text(t) => Some(t.bounds),
            _ => None,
        })
        .collect();
    for item in &mut panel.items {
        let Item::Line {
            path,
            stroke,
            role: Role::Grid | Role::Baseline,
            ..
        } = item
        else {
            continue;
        };
        let [Cmd::M { p: [x, top] }, Cmd::L { p: [x1, bottom] }] = path.cmds[..] else {
            continue;
        };
        if x != x1 || bottom <= top {
            continue;
        }
        let reach = TEXT_GAP + stroke.width / 2.0;
        let mut gaps: Vec<[f64; 2]> = boxes
            .iter()
            .filter(|b| {
                x >= b.x - reach && x <= b.right() + reach && b.bottom() + TEXT_GAP > top && b.y - TEXT_GAP < bottom
            })
            .map(|b| [b.y - TEXT_GAP, b.bottom() + TEXT_GAP])
            .collect();
        if gaps.is_empty() {
            continue;
        }
        gaps.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut broken = Path::new();
        let mut from = top;
        for [start, end] in gaps.into_iter().chain([[bottom, bottom]]) {
            let to = start.min(bottom);
            // A stub shorter than a pixel reads as a speck, not a line.
            if to - from >= 1.0 {
                broken.move_to([x, from]);
                broken.line_to([x, to]);
            }
            from = from.max(end);
        }
        *path = broken;
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_points(
    ctx: &mut Ctx,
    read: &Read,
    p: &Parsed,
    members: &Members,
    plot: Rect,
    xl: &XLayout,
    ymap: &impl Fn(f64) -> f64,
    items: &mut Vec<Item>,
    observations: &mut Vec<Observation>,
    taken: &mut Vec<Rect>,
) {
    let block = ctx.block;
    let x_readout = block.readout_format(&block.x, ctx.locale);
    let y_readout = block.readout_format(&block.y, ctx.locale);
    let size_readout = block.readout_format(&block.size_axis(), ctx.locale);
    let max_size = if block.encoding.size.is_some() {
        bubble_max(ctx, p)
    } else {
        0.0
    };
    // Every point in a series uses the same colour. Convert Oklch once per series,
    // rather than repeating the transfer function for every visible source point.
    let colors: Vec<_> = (0..ctx.series.len())
        .map(|s| Style::rgba(ctx.series_color(s)))
        .collect();
    let window = xl.scale.as_ref().map(|s| s.domain);
    let labelled = block.encoding.label.is_some() || block.highlight.is_some();
    let mut anchors: Vec<(usize, [f64; 2], f64)> = Vec::new();
    let automatic =
        block.style.points.is_none() && read.points > crate::limits::max(block.kind.name(), "all-points observations");
    // Reduction needs each point's actual radius (the hit radius has a minimum), but
    // unselected points need no paint item. Indices preserve the source observation order.
    let mut points: Vec<(usize, f64)> = Vec::new();
    let span = window.filter(|_| members.x_ordered).map_or(0..members.obs.len(), |w| {
        members.obs.partition_point(|&i| p.obs[i].x < w[0])..members.obs.partition_point(|&i| p.obs[i].x <= w[1])
    });
    if ctx.series.len() == 1 && !ctx.is_hidden(0) {
        if automatic {
            points.reserve(span.len());
        } else {
            items.reserve(span.len());
        }
        observations.reserve(span.len());
    }
    for (&index, readout) in members.obs[span.clone()].iter().zip(&members.readouts[span]) {
        let o = &p.obs[index];
        let Some(v) = o.y else { continue };
        if ctx.is_hidden(o.series) {
            continue;
        }
        if let Some(w) = window
            && (o.x < w[0] || o.x > w[1])
        {
            continue;
        }
        let center = [xl.map(o.x), ymap(v)];
        let r = match o.size {
            Some(s) if max_size > 0.0 => BUBBLE_MAX * (s / max_size).sqrt(),
            Some(_) => 0.0,
            None => POINT_RADIUS,
        };
        if r > 0.0 {
            if automatic {
                points.push((observations.len(), r));
            } else {
                items.push(point_item(
                    center,
                    r,
                    o.series,
                    observations.len(),
                    colors[o.series],
                    ctx.series[o.series].shape,
                ));
            }
        }
        if labelled {
            anchors.push((o.row, center, r));
        }
        observations.push(Observation {
            row: Some(o.row),
            dots: Vec::new(),
            series: Some(o.series),
            at: center,
            wedge: None,
            area: None,
            radius: r.max(POINT_RADIUS),
            x: center[0],
            readout: readout
                .get_or_init(|| point_readout(ctx, o, v, [&x_readout, &y_readout, &size_readout]).into())
                .clone(),
        });
    }
    if automatic {
        // One source point per occupied CSS-pixel cell and series, plus each series'
        // x/y/radius extremes. Selection is stable in source order; readouts stay complete.
        let mut keep = vec![false; points.len()];
        let mut cells = members.pixel_cells.borrow_mut();
        cells.clear();
        let mut extremes = vec![[None; 6]; ctx.series.len()];
        // Adjacent points often occupy the same cell. Merge their winner once per run;
        // the map still merges later visits to that cell, retaining the first radius tie.
        let mut pending: Option<((usize, i64, i64), usize)> = None;
        let mut commit = |(key, index)| {
            let entry = cells.entry(key).or_insert(index);
            if points[index].1 > points[*entry].1 {
                *entry = index;
            }
        };
        for (i, &(observation, radius)) in points.iter().enumerate() {
            let point = &observations[observation];
            let center = point.at;
            let s = point.series.expect("point series");
            let key = (s, center[0].floor() as i64, center[1].floor() as i64);
            if let Some((cell, kept)) = &mut pending
                && *cell == key
            {
                if radius > points[*kept].1 {
                    *kept = i;
                }
            } else if let Some(run) = pending.replace((key, i)) {
                commit(run);
            }
            for (k, value) in [center[0], -center[0], center[1], -center[1], radius, -radius]
                .into_iter()
                .enumerate()
            {
                if extremes[s][k].is_none_or(|(_, v)| value < v) {
                    extremes[s][k] = Some((i, value));
                }
            }
        }
        if let Some(run) = pending {
            commit(run);
        }
        for &i in cells.values() {
            keep[i] = true;
        }
        let occupied = cells.len();
        if cells.capacity() > occupied.saturating_mul(4).max(16) {
            cells.shrink_to(occupied);
        }
        cells.clear();
        for (i, _) in extremes.into_iter().flatten().flatten() {
            keep[i] = true;
        }
        let total = points.len();
        let kept = keep.iter().filter(|&&v| v).count();
        items.reserve(kept);
        for ((observation, radius), keep) in points.into_iter().zip(keep) {
            if keep {
                let point = &observations[observation];
                let s = point.series.expect("point series");
                items.push(point_item(
                    point.at,
                    radius,
                    s,
                    observation,
                    colors[s],
                    ctx.series[s].shape,
                ));
            }
        }
        ctx.check(
            "decimation", Level::Info,
            format!("Scatter draws {kept} representatives from {total} points: one per occupied pixel cell and series, plus extremes"),
            None, Some("chart.style.points".into()), Some(plot),
            json!({"perPixel": total as f64 / (plot.w * plot.h).max(1.0), "kept": kept, "total": total,
                "automatic": true, "method": "pixel-cell", "ceiling": crate::limits::max(block.kind.name(), "all-points observations")}),
        );
    }
    if labelled {
        place::point_labels(ctx, plot, &anchors, items, taken);
    }
}

/// Build a scatter mark at its actual radius, retaining its complete-source observation index.
fn point_item(
    center: [f64; 2],
    radius: f64,
    series: usize,
    observation: usize,
    color: crate::geometry::Rgba,
    shape: Option<Shape>,
) -> Item {
    Item::Point {
        center,
        radius,
        shape: shape.unwrap_or(Shape::Circle),
        fill: shape.is_none().then(|| Fill::solid(color.with_alpha(POINT_OPACITY))),
        stroke: shape.map(|_| Stroke::solid(color, 1.5)),
        role: Role::Mark,
        series: Some(series),
        obs: Some(observation),
    }
}

/// The largest bubble size the size scale covers: this chart's rows, or every frame of a playback
/// (FEA-BR-31).
fn bubble_max(ctx: &Ctx, p: &Parsed) -> f64 {
    p.obs
        .iter()
        .filter_map(|o| o.size)
        .fold(ctx.wanted().size.unwrap_or(0.0), f64::max)
}

/// The smallest positive bubble size the size key shows: this chart's rows, or every frame of a
/// playback (FEA-BR-31).
fn bubble_min(ctx: &Ctx, p: &Parsed) -> Option<f64> {
    p.obs
        .iter()
        .filter_map(|o| o.size)
        .filter(|s| *s > 0.0)
        .chain(ctx.wanted().size_min)
        .reduce(f64::min)
}

/// A field name read as a title: separators become spaces and the first letter is capitalised.
pub(super) fn readable_name(field: &str) -> String {
    let spaced = field.trim().replace(['_', '-'], " ");
    let mut chars = spaced.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// The size key under the plot (FEA-CH-07): a title naming the measure (`axes.size.label`, else
/// the field read as a name) and the smallest, middle and largest bubble, each with its value in
/// the axis's format and unit and all three written one way, drawn as the marks are (the series colour, one series) or outlined
/// (several). The title shares the circles' row when the row fits the figure, so the key costs
/// one row of circles and not two. A chart whose bubbles are all one size shows that one circle.
fn bubble_key(ctx: &Ctx, p: &Parsed, top: f64) -> (Vec<Item>, f64) {
    let field = ctx.block.encoding.size.clone().unwrap_or_default();
    let axis = ctx.block.size_axis();
    let title = axis.label.clone().unwrap_or_else(|| readable_name(&field));
    let max_size = bubble_max(ctx, p);
    let min_size = bubble_min(ctx, p).map_or(max_size, |m| m.min(max_size));
    let mut f = ctx.block.label_format(&axis, ctx.locale);
    // One way of writing the whole key: when the largest value takes a scale word, the smallest
    // takes one from 1,000 too. A plain "4,607" beside "715 million" and "1.43 billion" read as a
    // stray number, not as the key's low end (Daniel, 15 Nov 2026).
    if f.kind == Kind::Auto && max_size.abs() >= 10_000.0 {
        f.kind = Kind::NumberCompact;
    }
    let lh = ctx.line_height();
    let r = BUBBLE_MAX;
    let mut values = vec![max_size];
    if min_size < max_size {
        values = vec![min_size, min_size + (max_size - min_size) / 2.0, max_size];
    }
    let radius_of = |v: f64| {
        if max_size > 0.0 {
            (r * (v / max_size).sqrt()).max(1.5)
        } else {
            1.5
        }
    };
    // What each circle and its value take in a row, and where the title sits against them.
    let widths: Vec<f64> = values
        .iter()
        .map(|v| 2.0 * radius_of(*v) + 5.0 + ctx.text_width(&numbers::value(*v, &f)) + 16.0)
        .collect();
    let title_width = ctx.text_width(&title);
    let inline = 2.0 + title_width + 14.0 + widths.iter().sum::<f64>() - 16.0 <= ctx.opts.width;
    let (cy, height, mut at) = if inline {
        (top + 6.0 + r, 6.0 + 2.0 * r + 4.0, 2.0 + title_width + 14.0)
    } else {
        (top + 8.0 + lh + 6.0 + r, 8.0 + lh + 6.0 + 2.0 * r + 4.0, 2.0)
    };
    let title_item = if inline {
        ctx.text_mid(
            &title,
            2.0,
            cy,
            Anchor::Start,
            Face::Data,
            Role::Key,
            ctx.style.ink_soft,
        )
    } else {
        ctx.text(
            &title,
            2.0,
            top + 8.0 + ctx.style.font.baseline_offset(ctx.size()),
            Anchor::Start,
            Face::Data,
            Role::Key,
            ctx.style.ink_soft,
        )
    };
    let mut items = vec![Item::Text(title_item)];
    let like_marks = p.obs.iter().all(|o| o.series == 0);
    let first = at;
    for v in values {
        let radius = radius_of(v);
        let label = numbers::value(v, &f);
        let text = ctx.text_mid(
            &label,
            at + 2.0 * radius + 5.0,
            cy,
            Anchor::Start,
            Face::Figures,
            Role::Key,
            ctx.style.ink_soft,
        );
        let end = text.bounds.right() + 16.0;
        // A value that would run past the figure is left out rather than clipped.
        if end - 16.0 > ctx.opts.width && at > first {
            break;
        }
        items.push(Item::Point {
            center: [at + radius, cy],
            radius,
            shape: Shape::Circle,
            fill: like_marks.then(|| Fill::solid(Style::rgba(ctx.series_color(0)).with_alpha(POINT_OPACITY))),
            stroke: (!like_marks).then(|| Stroke::solid(Style::rgba(ctx.style.ink_soft), 1.0)),
            role: Role::Key,
            series: None,
            obs: None,
        });
        items.push(Item::Text(text));
        at = end;
    }
    (items, height)
}

#[cfg(test)]
mod tests {
    use crate::block::ChartBlock;
    use crate::geometry::{Item, Rect, Role, Text};
    use crate::layout::{ChartTheme, Options, layout};
    use serde_json::json;

    fn opts(width: f64) -> Options {
        Options {
            width,
            ..Options::default()
        }
    }

    #[test]
    fn a_two_series_line_is_named_at_its_ends() {
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "color": "s"}}),
        )
        .unwrap();
        let rows: Vec<_> = (1..=12)
            .flat_map(|m| {
                [
                    json!({"m": format!("2024-{m:02}"), "v": m * 10, "s": "North"}),
                    json!({"m": format!("2024-{m:02}"), "v": 130 - m * 10, "s": "South"}),
                ]
            })
            .collect();
        let g = layout(&block, &rows, &ChartTheme::default(), &opts(640.0));
        assert!(g.failure.is_none());
        assert!(g.legend.is_none(), "names fit at the line ends");
        let names: Vec<_> = g
            .items()
            .filter_map(|i| {
                if let Item::Text(t) = i
                    && t.role == Role::Label
                {
                    Some(t.text.clone())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(names, ["North", "South"]);
        assert_eq!(g.panels[0].observations.len(), 12, "one readout per month");
        assert_eq!(g.panels[0].observations[0].readout.lines.len(), 2);
        assert!(g.width - g.panels[0].plot.right() >= 9.0);
        assert!(
            (g.width / g.height - 2.0).abs() < 0.3,
            "calendar lines keep 2:1, {} x {}",
            g.width,
            g.height
        );
    }

    #[test]
    fn line_and_area_leave_room_for_the_selected_endpoint_halo() {
        for kind in ["line", "area"] {
            let block =
                ChartBlock::from_value(&json!({"type": kind, "title": "T", "encoding": {"x": "x", "y": "y"}})).unwrap();
            let rows = [json!({"x": 1, "y": 4}), json!({"x": 2, "y": 7})];
            for width in [318.0, 640.0] {
                let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
                assert!(g.failure.is_none());
                let panel = &g.panels[0];
                assert!(g.width - panel.plot.right() >= 9.0);
                assert!(panel.observations.last().unwrap().at[0] <= panel.plot.right());
            }
        }
    }

    #[test]
    fn a_dense_line_draws_thinner_than_a_sparse_one() {
        let block =
            ChartBlock::from_value(&json!({"type": "line", "title": "T", "encoding": {"x": "x", "y": "y"}})).unwrap();
        let width_of = |n: usize| {
            let rows: Vec<_> = (0..n).map(|i| json!({"x": i, "y": (i % 7) as f64})).collect();
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(640.0));
            assert!(g.failure.is_none());
            let widths: Vec<f64> = g
                .items()
                .filter_map(|i| match i {
                    Item::Line {
                        stroke,
                        role: Role::Mark,
                        ..
                    } => Some(stroke.width),
                    _ => None,
                })
                .collect();
            assert!(!widths.is_empty());
            widths[0]
        };
        assert_eq!(width_of(24), super::LINE_WIDTH, "points far apart keep the full weight");
        assert_eq!(
            width_of(4000),
            super::DENSE_LINE_WIDTH,
            "a pixel or less apart draws thin"
        );
        let middle = width_of(300);
        assert!(
            super::DENSE_LINE_WIDTH < middle && middle < super::LINE_WIDTH,
            "{middle}"
        );
    }

    #[test]
    fn zoom_labels_the_series_own_values_and_never_a_windows_extremes() {
        let block =
            ChartBlock::from_value(&json!({"type": "line", "title": "T", "encoding": {"x": "x", "y": "y"}})).unwrap();
        let rows: Vec<_> = (0..120)
            .map(|i| {
                let y = match i {
                    10 => -50.0,
                    100 => 80.0,
                    _ => (i % 9) as f64,
                };
                json!({"x": i, "y": y})
            })
            .collect();
        let values = |zoom: Option<[f64; 2]>| -> Vec<String> {
            let g = layout(&block, &rows, &ChartTheme::default(), &Options { zoom, ..opts(640.0) });
            assert!(g.failure.is_none());
            g.items()
                .filter_map(|i| match i {
                    Item::Text(t) if t.role == Role::Label && t.mark.is_some() => Some(t.text.clone()),
                    _ => None,
                })
                .collect()
        };
        let whole = values(None);
        assert!(
            whole.iter().any(|t| t.contains("50")),
            "the lowest value is labelled: {whole:?}"
        );
        assert!(
            whole.iter().any(|t| t.contains("80")),
            "the highest value is labelled: {whole:?}"
        );
        let inside = values(Some([40.0, 80.0]));
        assert!(
            inside.is_empty(),
            "a window without the series' own values names none: {inside:?}"
        );
        let low = values(Some([5.0, 60.0]));
        assert_eq!(low.len(), 1, "only the series' lowest value is in this window: {low:?}");
        assert!(low[0].contains("50"), "{low:?}");
    }

    #[test]
    fn a_value_label_away_from_its_point_gets_a_leader_to_its_near_side() {
        use crate::geometry::Rect;
        use crate::layout::place::leader_end;
        let label = Rect::new(100.0, 50.0, 40.0, 12.0);
        assert_eq!(leader_end([103.0, 47.0], label), None, "within 4 px of its point");
        assert_eq!(
            leader_end([170.0, 56.0], label),
            Some([142.0, 56.0]),
            "point to its right"
        );
        assert_eq!(leader_end([60.0, 70.0], label), Some([98.0, 56.0]), "point to its left");
        assert_eq!(leader_end([120.0, 90.0], label), Some([120.0, 64.0]), "point below it");
    }

    #[test]
    fn repeated_bar_slots_reject_the_chart() {
        let block =
            ChartBlock::from_value(&json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}})).unwrap();
        let rows = vec![json!({"c": "A", "v": 1}), json!({"c": "A", "v": 2})];
        let g = layout(&block, &rows, &ChartTheme::default(), &opts(640.0));
        assert_eq!(
            g.failure.as_deref(),
            Some("Cannot draw these bars · Repeated observations")
        );
        assert!(g.checks.iter().any(|c| c.kind == "ambiguous-bar-slot"));
    }

    #[test]
    fn bars_start_at_zero_and_round_only_their_value_end() {
        let block =
            ChartBlock::from_value(&json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}})).unwrap();
        let rows = vec![
            json!({"c": "A", "v": 10}),
            json!({"c": "B", "v": -5}),
            json!({"c": "C", "v": 0}),
        ];
        let g = layout(&block, &rows, &ChartTheme::default(), &opts(640.0));
        let bars: Vec<_> = g
            .items()
            .filter_map(|i| {
                if let Item::Rect {
                    rect,
                    radius,
                    role: Role::Mark,
                    ..
                } = i
                {
                    Some((*rect, *radius))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(bars.len(), 2, "a zero draws no length");
        assert!(bars[0].1[2] == 0.0 && bars[0].1[0] > 0.0);
        assert!(bars[1].1[0] == 0.0 && bars[1].1[2] > 0.0);
        assert_eq!(g.panels[0].observations.len(), 3, "the zero stays inspectable");
    }

    #[test]
    fn non_numeric_text_is_an_error_naming_the_row() {
        let block =
            ChartBlock::from_value(&json!({"type": "line", "title": "T", "encoding": {"x": "x", "y": "y"}})).unwrap();
        let rows = vec![json!({"x": 1, "y": "1,234"}), json!({"x": 2, "y": 3})];
        let g = layout(&block, &rows, &ChartTheme::default(), &opts(640.0));
        assert!(g.failure.is_some());
        let check = g.checks.iter().find(|c| c.kind == "non-numeric").unwrap();
        assert_eq!(check.path.as_deref(), Some("rows[0].y"));
    }

    #[test]
    fn scatter_bubbles_scale_by_area() {
        let block = ChartBlock::from_value(
            &json!({"type": "scatter", "title": "T", "encoding": {"x": "x", "y": "y", "size": "s"}}),
        )
        .unwrap();
        let rows = vec![
            json!({"x": 1, "y": 1, "s": 100}),
            json!({"x": 2, "y": 2, "s": 25}),
            json!({"x": 3, "y": 3, "s": 0}),
        ];
        let g = layout(&block, &rows, &ChartTheme::default(), &opts(640.0));
        let radii: Vec<f64> = g.panels[0]
            .items
            .iter()
            .filter_map(|i| {
                if let Item::Point {
                    radius,
                    role: Role::Mark,
                    ..
                } = i
                {
                    Some(*radius)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(radii, [16.0, 8.0]);
        assert_eq!(g.panels[0].observations.len(), 3);
        assert!(!g.figure_items.is_empty(), "size reference under the plot");
    }

    #[test]
    fn long_nominal_names_turn_bars_horizontal_and_the_chart_grows() {
        let block =
            ChartBlock::from_value(&json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}})).unwrap();
        let names = [
            "Accounts payable",
            "Brand marketing",
            "Customer success",
            "Data platform",
            "Engineering tools",
            "Facilities and office",
            "Government relations",
            "Human resources",
            "Internal audit",
            "Joint ventures",
            "Knowledge base",
            "Legal and compliance",
            "Mobile apps",
            "Network operations",
        ];
        let rows: Vec<_> = names.iter().enumerate().map(|(i, n)| json!({"c": n, "v": i})).collect();
        let g = layout(&block, &rows, &ChartTheme::default(), &opts(640.0));
        assert!(g.height > 640.0 / (16.0 / 9.0));
        let marks = g
            .items()
            .filter(|i| matches!(i, Item::Rect { role: Role::Mark, .. }))
            .count();
        assert_eq!(marks, 13);
    }

    fn tick_texts(g: &crate::geometry::Geometry) -> Vec<String> {
        g.items()
            .filter_map(|i| match i {
                Item::Text(t) if matches!(t.role, Role::Tick | Role::AxisLabel) => Some(t.text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_scatter_axis_ends_just_past_its_data_and_the_size_key_shares_a_row() {
        let block = ChartBlock::from_value(&json!({
            "type": "scatter", "title": "T",
            "encoding": {"x": "seats", "y": "features", "size": "acv"}
        }))
        .unwrap();
        let rows: Vec<_> = [(12, 4, 9), (80, 12, 70), (160, 14, 98), (260, 17, 150), (400, 22, 310)]
            .iter()
            .map(|(x, y, s)| json!({"seats": x, "features": y, "acv": s}))
            .collect();
        for width in [390.0, 720.0] {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            assert!(g.failure.is_none());
            let ticks = tick_texts(&g);
            assert!(ticks.contains(&"400".to_string()), "{width}: {ticks:?}");
            assert!(
                !ticks.contains(&"500".to_string()) && !ticks.contains(&"600".to_string()),
                "{width}: {ticks:?}"
            );
            // One series: the key's circles are the marks' fill, and the title stands on their row.
            let circles: Vec<_> = g
                .figure_items
                .iter()
                .filter_map(|i| match i {
                    Item::Point { center, fill, .. } => Some((center[1], fill.is_some())),
                    _ => None,
                })
                .collect();
            assert!(
                !circles.is_empty() && circles.iter().all(|(_, filled)| *filled),
                "{circles:?}"
            );
            let title = g
                .figure_items
                .iter()
                .find_map(|i| match i {
                    Item::Text(t) if t.role == Role::Key => Some(t.bounds),
                    _ => None,
                })
                .unwrap();
            assert!(
                (title.y + title.h / 2.0 - circles[0].0).abs() < 1.0,
                "title on the circles' row"
            );
        }
    }

    /// Phone widths: the canvases a 390 px page gives a chart, and the page itself.
    const PHONE: [f64; 3] = [318.0, 358.0, 390.0];

    /// Ten survey questions, net agreement: eight bars right of zero and two left of it.
    fn diverging_bars() -> (ChartBlock, Vec<serde_json::Value>) {
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "T", "encoding": {"x": "topic", "y": "net"},
            "axes": {"y": {"label": "Agree minus disagree (points)"}}
        }))
        .unwrap();
        let rows = [
            ("Safe to disagree with manager", 69),
            ("Work ties to company goals", 69),
            ("Have the tools I need", 62),
            ("Would recommend us", 61),
            ("Work-life balance", 39),
            ("Useful monthly feedback", 34),
            ("Decisions explained in time", 17),
            ("Pay is fair", 5),
            ("Career path in two years", -1),
            ("Meetings worth the time", -11),
        ]
        .iter()
        .map(|(topic, net)| json!({"topic": topic, "net": net}))
        .collect();
        (block, rows)
    }

    /// The change since last year: five bars each side of zero.
    fn change_bars() -> (ChartBlock, Vec<serde_json::Value>) {
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "T", "encoding": {"x": "topic", "y": "change"},
            "axes": {"y": {"label": "Change vs 2025 (points)"}}
        }))
        .unwrap();
        let rows = [
            ("Pay is fair", 8),
            ("Have the tools I need", 7),
            ("Meetings worth the time", 6),
            ("Useful monthly feedback", 5),
            ("Would recommend us", 2),
            ("Safe to disagree with manager", -1),
            ("Decisions explained in time", -3),
            ("Career path in two years", -7),
            ("Work ties to company goals", -7),
            ("Work-life balance", -7),
        ]
        .iter()
        .map(|(topic, change)| json!({"topic": topic, "change": change}))
        .collect();
        (block, rows)
    }

    /// Agreement out of 100 at 4:5, where a bar's value end meets the 50 gridline.
    fn agreement_bars() -> (ChartBlock, Vec<serde_json::Value>) {
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "T", "encoding": {"x": "question", "y": "agree"},
            "axes": {"x": {"label": "Question"}, "y": {"label": "Agree (%)", "min": 0, "max": 100}},
            "style": {"ratio": "4:5"}
        }))
        .unwrap();
        let rows = [
            ("Understand how work fits goals", 81),
            ("Safe to raise problems", 77),
            ("Have tools to do the job", 74),
            ("Would recommend company", 71),
            ("Monthly useful feedback", 58),
            ("Work-life balance", 63),
            ("Decisions explained in time", 44),
            ("Pay is fair", 35),
            ("Realistic career path", 39),
            ("Meetings are a good use of time", 28),
        ]
        .iter()
        .map(|(question, agree)| json!({"question": question, "agree": agree}))
        .collect();
        (block, rows)
    }

    /// Bars by observation, value labels by the observation they print.
    type BarsAndValues<'a> = (Vec<(usize, Rect)>, Vec<(usize, &'a Text)>);
    /// A vertical line's segments, each top to bottom.
    type Rings = Vec<Vec<[f64; 2]>>;

    /// Each bar with its observation, and each value label with the observation it prints.
    fn bars_and_values(panel: &crate::geometry::Panel) -> BarsAndValues<'_> {
        let bars = panel
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Rect {
                    rect,
                    role: Role::Mark,
                    obs: Some(o),
                    ..
                } => Some((*o, *rect)),
                _ => None,
            })
            .collect();
        let values = panel
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text(t) if t.role == Role::Label => {
                    let row: usize = t.mark.as_deref()?.strip_prefix("row:")?.parse().ok()?;
                    let o = panel.observations.iter().position(|o| o.row == Some(row))?;
                    Some((o, t))
                }
                _ => None,
            })
            .collect();
        (bars, values)
    }

    fn names(panel: &crate::geometry::Panel) -> Vec<Rect> {
        panel
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text(t) if t.role == Role::AxisLabel && t.bounds.bottom() <= panel.plot.bottom() => {
                    Some(t.bounds)
                }
                _ => None,
            })
            .collect()
    }

    /// Parts of `from..to` that no segment of a vertical line covers.
    fn uncovered(rings: &[Vec<[f64; 2]>], from: f64, to: f64) -> Vec<[f64; 2]> {
        let mut spans: Vec<[f64; 2]> = rings.iter().map(|r| [r[0][1], r[r.len() - 1][1]]).collect();
        spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut out = Vec::new();
        let mut at = from;
        for [a, b] in spans.into_iter().chain([[to, to]]) {
            if a.min(to) > at {
                out.push([at, a.min(to)]);
            }
            at = at.max(b);
        }
        out
    }

    #[test]
    fn horizontal_bar_lines_break_only_where_text_sits() {
        for (chart, (block, rows)) in [diverging_bars(), change_bars(), agreement_bars()]
            .into_iter()
            .enumerate()
        {
            // Phone widths, and the 560 px a desktop report column gives a chart.
            for width in PHONE.into_iter().chain([560.0]) {
                let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
                assert!(g.failure.is_none());
                let panel = &g.panels[0];
                let texts: Vec<Rect> = panel
                    .items
                    .iter()
                    .filter_map(|i| match i {
                        Item::Text(t) => Some(t.bounds),
                        _ => None,
                    })
                    .collect();
                let (bars, values) = bars_and_values(panel);
                let lines: Vec<(Role, f64, Rings)> = panel
                    .items
                    .iter()
                    .filter_map(|i| match i {
                        Item::Line { path, role, stroke, .. } if matches!(role, Role::Grid | Role::Baseline) => {
                            Some((*role, stroke.width / 2.0, path.flatten(0.25)))
                        }
                        _ => None,
                    })
                    .collect();
                assert!(lines.iter().any(|(role, ..)| *role == Role::Baseline));
                if width < 480.0 && chart < 2 {
                    let names = names(panel);
                    assert_eq!(names.len(), 10);
                    let crossed = lines
                        .iter()
                        .filter(|(_, _, rings)| {
                            names.iter().any(|n| n.x < rings[0][0][0] && rings[0][0][0] < n.right())
                        })
                        .count();
                    assert!(
                        crossed >= 2,
                        "chart {chart} at {width}: the zero line and a gridline pass names"
                    );
                }
                for (role, half, rings) in &lines {
                    let x = rings[0][0][0];
                    let near: Vec<&Rect> = texts
                        .iter()
                        .filter(|t| x >= t.x - 2.0 - half && x <= t.right() + 2.0 + half)
                        .collect();
                    for ring in rings {
                        for t in &texts {
                            assert!(
                                !crate::layout::place::segment_touches_rect(ring[0], ring[1], &t.inflate(*half)),
                                "chart {chart} at {width}: a {role:?} line at x={x} runs through text at {t:?}"
                            );
                        }
                    }
                    // Every break is room round text the line would cross; none is a stray cut.
                    // Only stubs under a pixel may go too.
                    let mut stray = uncovered(rings, panel.plot.y, panel.plot.bottom());
                    for t in &near {
                        stray = stray
                            .into_iter()
                            .flat_map(|[a, b]| [[a, b.min(t.y - 2.0)], [a.max(t.bottom() + 2.0), b]])
                            .filter(|[a, b]| b - a >= 1.0)
                            .collect();
                    }
                    assert!(
                        stray.is_empty(),
                        "chart {chart} at {width}: a {role:?} line at x={x} breaks at {stray:?} with no text there"
                    );
                    // A line no value sits across runs through every bar row.
                    let crosses_value = values.iter().any(|(_, t)| near.contains(&&t.bounds));
                    if !crosses_value {
                        for (_, bar) in &bars {
                            assert!(
                                uncovered(rings, bar.y, bar.bottom()).is_empty(),
                                "chart {chart} at {width}: a {role:?} line at x={x} leaves the row of {bar:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn on_a_phone_a_horizontal_bar_value_keeps_to_its_own_row() {
        for (chart, (block, rows)) in [diverging_bars(), change_bars(), agreement_bars()]
            .into_iter()
            .enumerate()
        {
            for width in PHONE {
                let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
                let panel = &g.panels[0];
                let band = panel.plot.h / 10.0;
                let (bars, values) = bars_and_values(panel);
                assert_eq!(values.len(), 10, "chart {chart} at {width}: {:?}", g.dropped);
                let zero = panel
                    .items
                    .iter()
                    .find_map(|i| match i {
                        Item::Line {
                            path,
                            role: Role::Baseline,
                            ..
                        } => Some(path.flatten(0.25)[0][0][0]),
                        _ => None,
                    })
                    .unwrap();
                for (o, t) in values {
                    let bar = bars.iter().find(|(b, _)| *b == o).unwrap().1;
                    let row = ((bar.center()[1] - panel.plot.y) / band).floor();
                    let own = Rect::new(panel.plot.x, panel.plot.y + row * band, panel.plot.w, band);
                    assert!(
                        t.bounds.inside(&own, 0.01),
                        "chart {chart} at {width}: '{}' {:?} leaves its row {own:?}",
                        t.text,
                        t.bounds
                    );
                    let middle = t.bounds.center()[1];
                    assert!(
                        bar.y <= middle && middle <= bar.bottom(),
                        "chart {chart} at {width}: '{}' is not level with its bar",
                        t.text
                    );
                    // Past the value end: right of a bar above zero, left of one below it.
                    let inside = t.bounds.inside(&bar, 0.0);
                    if !inside && bar.x < zero - 1.0 {
                        assert!(t.bounds.right() <= bar.x, "chart {chart} at {width}: '{}'", t.text);
                    } else if !inside {
                        assert!(t.bounds.x >= bar.right(), "chart {chart} at {width}: '{}'", t.text);
                    }
                }
            }
        }
    }

    #[test]
    fn a_value_where_a_gridline_meets_its_bar_end_stays_beside_it() {
        let (block, rows) = agreement_bars();
        for width in PHONE {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            let panel = &g.panels[0];
            let (bars, values) = bars_and_values(panel);
            assert!(g.dropped.is_empty(), "{width}: {:?}", g.dropped);
            assert_eq!(values.len(), 10, "{width}");
            let (o, t) = values.iter().find(|(_, t)| t.text == "44").unwrap();
            let bar = bars.iter().find(|(b, _)| b == o).unwrap().1;
            assert!(
                (t.bounds.x - bar.right() - 3.0).abs() < 0.01,
                "{width}: '44' sits just past its bar end"
            );
        }
    }

    #[test]
    fn an_interval_unit_axis_follows_its_data_instead_of_zero() {
        let block = ChartBlock::from_value(&json!({
            "type": "line", "title": "T", "encoding": {"x": "d", "y": "t"},
            "axes": {"y": {"unit": "°C"}}
        }))
        .unwrap();
        let rows: Vec<_> = (0..7)
            .map(|d| json!({"d": format!("2025-03-{:02}", d + 1), "t": 8 + d * 2}))
            .collect();
        let g = layout(&block, &rows, &ChartTheme::default(), &opts(640.0));
        let ticks = tick_texts(&g);
        assert!(!ticks.iter().any(|t| t.starts_with('0')), "no zero tick: {ticks:?}");
        assert!(ticks.iter().any(|t| t.starts_with("20")), "{ticks:?}");
    }
}
