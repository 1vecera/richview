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
    Role, Shape, Stroke, Swatch, Targeting, XScale, YSpan,
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

    let drawn_rows: Vec<(usize, usize)> = parsed.drawn().map(|o| (o.row, o.series)).collect();
    ctx.resolve_emphasis(&drawn_rows);
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
    // A shown series the author's highlight mutes is never named, so it takes no room (TYP-78).
    let visible_names: Vec<String> = (0..ctx.series.len())
        .filter(|&s| !ctx.mutes_series(s) || ctx.is_hidden(s))
        .map(|s| ctx.series[s].name.clone())
        .collect();
    let end_width = visible_names.iter().map(|n| ctx.text_width(n)).fold(0.0, f64::max) + 10.0;
    // Lines may be named in a column at their right ends. Areas are named inside the plot or in
    // the legend: their names once stood in the value axis' margin, where they read as stray
    // axis labels (Daniel, 16 Nov 2026).
    let mut end_names = kind == ChartType::Line && multi && end_width <= panel_width * 0.3;
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
    // Series are named inside the plot, each beside its own line, when every name of every panel
    // finds a clear place there; the plot then takes the width of the name column (FEA-CH-27).
    // This first search knows no value or annotation yet; the panel's own search, once those
    // stand, has the last word. Like the line-end decision it starts from the chart with every
    // series shown, so hiding a series cannot turn a legend into names.
    let mut inside_names = line_like && multi;
    if inside_names {
        'panels: for (fi, ((_, members), frame)) in facets.iter().zip(&frames).enumerate() {
            if parsed.failures[fi].is_some() {
                continue;
            }
            let (current, all_extent) = panel_extents[fi];
            let members = read.panels[fi].get_or_init(|| Members::new(ctx, parsed, members));
            for (extent, all_shown) in
                std::iter::once((all_extent, true)).chain((!ctx.opts.hidden.is_empty()).then_some((current, false)))
            {
                let plan = panel_layout(ctx, parsed, members, *frame, title_room, extent, false);
                let lines = series_lines(ctx, parsed, members, &plan.xl, &plan.fr.y, plan.fr.plot, all_shown);
                if place::inside_name_places(ctx, plan.fr.plot, &lines, &[], kind == ChartType::Area).is_none() {
                    inside_names = false;
                    break 'panels;
                }
            }
        }
    }
    if end_names {
        for (fi, ((_, members), frame)) in facets.iter().zip(&frames).enumerate() {
            if parsed.failures[fi].is_some() {
                continue;
            }
            let (current, all_extent) = panel_extents[fi];
            let members = read.panels[fi].get_or_init(|| Members::new(ctx, parsed, members));
            for extent in std::iter::once(all_extent).chain((!ctx.opts.hidden.is_empty()).then_some(current)) {
                let plan = panel_layout(ctx, parsed, members, *frame, title_room, extent, true);
                let plot = plan.fr.plot;
                // A column of names more than a quarter as wide as the plot beside it costs the
                // chart too much: the names then go to the legend row (chart design, 16 Nov 2026).
                if !place::line_names_fit(ctx, plot) || frame.right() - plot.right() > 0.25 * plot.w {
                    end_names = false;
                    break;
                }
            }
            if !end_names {
                break;
            }
        }
    }
    // Names inside the plot are tried first. A panel whose values and annotations leave some
    // name without a place says so, and the whole chart is painted again with its names at the
    // line ends or in the legend: names inside never cost a value.
    let ends_names = end_names;
    let mut figure_items = Vec::new();
    let (panels, legend, end_names) = loop {
        let end_names = inside_names || ends_names;
        let legend = (multi && !end_names).then(|| Legend {
            swatch: match kind {
                ChartType::Line => Swatch::Line,
                ChartType::Scatter => Swatch::Dot,
                _ => Swatch::Square,
            },
            // A muted series has no entry: the legend names what the chart is about (TYP-78).
            entries: (0..ctx.series.len()).filter(|&s| !ctx.mutes_series(s)).collect(),
            bottom: ctx.style.legend_bottom,
        });
        let marks = (
            ctx.checks.len(),
            ctx.dropped.len(),
            ctx.note.len(),
            ctx.derived.borrow().len(),
        );
        let buffers = inside_names.then(|| ctx.observations.clone());
        let mut named = true;
        let mut panels = Vec::new();
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
                inside_names,
                &mut named,
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
        if inside_names && !named {
            ctx.checks.truncate(marks.0);
            ctx.dropped.truncate(marks.1);
            ctx.note.truncate(marks.2);
            ctx.derived.borrow_mut().truncate(marks.3);
            if let Some(buffers) = buffers {
                ctx.observations = buffers;
            }
            inside_names = false;
            continue;
        }
        break (panels, legend, end_names);
    };
    let _ = end_names;
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
                let (value, label) = match self.rows.iter().find(|r| r.0 == s) {
                    Some((_, Some(v), _)) => (
                        numbers::value(*v, readout),
                        ctx.value_label(*v, readout, ctx.block.label_digits(&ctx.block.y)),
                    ),
                    Some((_, None, _)) => ("No observation".into(), None),
                    None if n_series > 1 => ("No observation".into(), None),
                    None => continue,
                };
                lines.push(ReadoutLine {
                    label,
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
            label: None,
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
            label: None,
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
            label: None,
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
            label: None,
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
                    label: None,
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

/// The reader's window over named positions: at least `least` wide, inside the axis.
fn named_window(zoom: Option<[f64; 2]>, extent: [f64; 2], least: f64) -> [f64; 2] {
    let Some([a, b]) = zoom.filter(|w| w[0].is_finite() && w[1].is_finite() && w[0] < w[1]) else {
        return extent;
    };
    let whole = extent[1] - extent[0];
    let span = (b - a).max(least).min(whole);
    let start = ((a + b) / 2.0 - span / 2.0).clamp(extent[0], extent[1] - span);
    [start, start + span]
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
                    y: None,
                    least: None,
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
                    y: None,
                    least: None,
                }),
                unfit: vec![],
                thinned: None,
            }
        }
        XModel::Category { names, reader, ordered } => {
            let n = names.len().max(1);
            // An area runs from the plot's left edge to its right, as it does over dates: its
            // first and last categories stand on the edges, and a step, not a band, separates
            // two of them (Daniel, 16 Nov 2026). Bars, and every mark that shares a bar's
            // category, keep a band each, half of it beyond the first and the last.
            let edge_to_edge = ctx.block.kind == ChartType::Area && n > 1;
            let extent = if edge_to_edge {
                [0.0, (n - 1) as f64]
            } else {
                [-0.5, n as f64 - 0.5]
            };
            // A line or an area over three names or more zooms like one over numbers or dates
            // (FEA-CH-49; Chart design, 16 November): its window counts positions and its ends
            // stand on the plot's edges, so a zoomed area still runs edge to edge. A settled
            // window ends on names and holds two at least (the runtime snaps it); a glide passes
            // through any window between. Bars and a line over two names keep every name in view.
            let least = 1.0;
            let zooms = matches!(ctx.block.kind, ChartType::Line | ChartType::Area) && n >= 3;
            let [a, b] = if zooms {
                named_window(ctx.opts.zoom, extent, least)
            } else {
                extent
            };
            let band = plot.w / (b - a);
            let x0 = plot.x - (a + 0.5) * band;
            let map = XMapping::Category { start: x0, band };
            let layout = axis::category_layout_with(reader, ordered.as_ref(), band, font, size, true);
            let mut unfit = Vec::new();
            let mut thinned = None;
            let lines: Vec<Vec<String>> = match layout {
                CategoryLayout::OneLine => reader.iter().map(|r| vec![r.clone()]).collect(),
                CategoryLayout::Wrapped(lines) => lines,
                CategoryLayout::Thinned { every, lines } => {
                    // A calendar step reads in its own unit; numbered names (decades, days of a
                    // trial) count as names, since their numbers need not step by one.
                    thinned = Some(match ordered.as_ref().map(|o| o.period) {
                        Some(axis::Period::Number) | None => format!("every {} name", axis::nth_printed(&lines)),
                        Some(period) => format!("every {every} {}s", period.noun()),
                    });
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
                .filter(|(x, _)| *x >= plot.x - 0.5 && *x <= plot.right() + 0.5)
                .collect();
            XLayout {
                mapping: map,
                ticks,
                band,
                scale: zooms.then_some(XScale {
                    domain: [a, b],
                    extent,
                    range: [plot.x, plot.right()],
                    calendar: false,
                    y: None,
                    least: Some(least),
                }),
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
    // The y title's box, as `paint_panel` sets it: the one text a bar's value can meet above.
    let y_title = block.y.label.as_ref().map(|label| {
        ctx.text(
            label,
            frame.x,
            top - lh / 2.0 - 6.0,
            Anchor::Start,
            Face::Data,
            Role::AxisTitle,
            ctx.style.ink,
        )
        .bounds
    });
    let mut value_room = 0.0;
    // Under an axis the author fixed, the plot starts this much lower instead.
    let mut top_room = 0.0;
    // A scatter plot zooms along y too, on its own window (FEA-CH-49): the whole axis is the zoom
    // limit, and a window inside it is drawn exactly. Panels with scales of their own share no
    // y window, so they zoom along x only.
    let zooms_y = scatter_zooms_y(ctx);
    let mut whole_y: Option<[f64; 2]> = None;
    for _ in 0..5 {
        let plot_h = (frame.bottom() - top - top_room - bottom_rows * lh - LABEL_GAP - x_title).max(MIN_PLOT * 0.5);
        let spec = ValueSpec {
            data: y_extent,
            zero,
            min: block.y.min,
            max: block.y.max,
            length: plot_h,
            horizontal: false,
            format: &y_format,
            font: &ctx.style.figures,
            size,
        };
        let room = if kind == ChartType::Scatter {
            point_room(ctx)
        } else {
            value_room
        };
        let mut y = axis::value_axis_room(&spec, room);
        if zooms_y {
            whole_y = Some([y.lo, y.hi]);
            if let Some(window) = super::kit::zoom_window(ctx.opts.zoom_y, [y.lo, y.hi]) {
                y = axis::value_axis_room(
                    &ValueSpec {
                        data: window,
                        min: Some(window[0]),
                        max: Some(window[1]),
                        ..spec
                    },
                    room,
                );
            }
        }
        // On a slide's stage the plot carries two to four gridlines (FEA-RP-65).
        let y = if ctx.style.few_gridlines { y.thinned(3) } else { y };
        let left = frame.x + (y.widest(&ctx.style.figures, size) + LABEL_GAP).max(inset[0]);
        // A series marker on the last x keeps its whole disk inside the chart.
        let edge = if ctx.series.iter().any(|s| s.shape.is_some()) {
            MARKER_RADIUS + MARKER_STROKE / 2.0 + 0.5
        } else {
            2.0
        };
        let mut right_pad = if end_names {
            end_gutter.max(inset[1])
        } else {
            inset[1].max(edge)
        };
        if matches!(kind, ChartType::Line | ChartType::Area) {
            // The nearest-x hover dot reaches 6px plus its 2px halo. Leave one more
            // CSS pixel for backing-canvas rounding at supported device scales.
            right_pad = right_pad.max(edge).max(9.0);
        }
        let plot = Rect::new(
            left,
            top + top_room,
            (frame.right() - right_pad - left).max(40.0),
            plot_h,
        );
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
        let mut settled = rows_needed == bottom_rows && left_over <= 0.5;
        if value_room == 0.0 {
            match bar_value_room(
                ctx,
                p,
                members,
                y_extent[1],
                &xl,
                &y,
                plot,
                y_title,
                frame.y + title_room,
            ) {
                Some(BarRoom::Axis(room)) => {
                    value_room = room;
                    settled = false;
                }
                Some(BarRoom::Plot(short)) => {
                    top_room += short;
                    settled = false;
                }
                _ => {}
            }
        }
        if left_over > 0.5 {
            inset[0] = (left - frame.x) + left_over;
        }
        bottom_rows = rows_needed;
        layout = Some((Frame { plot, y }, xl));
        if settled {
            break;
        }
    }
    let (fr, mut xl) = layout.expect("laid out");
    // The y axis the reader zooms along, bottom first (FEA-CH-49).
    if let (Some(extent), Some(scale)) = (whole_y, xl.scale.as_mut()) {
        scale.y = Some(YSpan {
            domain: [fr.y.lo, fr.y.hi],
            extent,
            range: [fr.plot.bottom(), fr.plot.y],
        });
    }
    PanelLayout {
        fr,
        xl,
        x_title_lines,
        bottom_rows,
        top,
    }
}

/// Whether this chart zooms along y on a window of its own, beside x (FEA-CH-49): a scatter whose
/// panels share one y axis.
fn scatter_zooms_y(ctx: &Ctx) -> bool {
    ctx.block.kind == ChartType::Scatter && !ctx.block.style.free_scales
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
    inside_names: bool,
    named: &mut bool,
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
    } = panel_layout(ctx, p, members, frame, title_room, y_extent, end_names && !inside_names);
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
    // An annotated scatter's points to name, which wait until the author's annotations stand.
    let mut named_points = None;
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
                inside_names,
                &mut labels_taken,
            );
        }
        ChartType::Bar => paint_bars(ctx, p, &obs, plot, &xl, &ymap, &fr.y, &mut items, &mut observations),
        ChartType::Scatter => {
            // A zoom along y shows the points inside its window and no others (FEA-CH-49).
            let y_window = xl
                .scale
                .as_ref()
                .and_then(|s| s.y)
                .filter(|y| y.domain != y.extent)
                .map(|y| y.domain);
            named_points = paint_points(
                ctx,
                read,
                p,
                members,
                plot,
                &xl,
                &ymap,
                y_window,
                &mut items,
                &mut observations,
                &mut labels_taken,
            )
        }
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
    let drawn = if ctx.block.annotations.is_empty() {
        place::Drawn::default()
    } else {
        place::annotations(
            ctx,
            plot,
            &xl_map(&xl, &p.x),
            &ymap,
            &mut items,
            &mut labels_taken,
            &obs_points(p, &obs, &xl, &ymap),
        )
    };
    // The author's annotations come before every other optional label (FEA-CH-29): the point
    // names here, then the values and the names inside the plot below.
    if let Some(anchors) = named_points {
        place::point_labels(ctx, plot, &anchors, &mut items, &mut labels_taken, &drawn);
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
        ChartType::Bar => {
            place::value_labels(
                ctx,
                facet,
                &mut panel,
                place::ValueLabels::Bars {
                    horizontal: false,
                    stacked: block.stack,
                },
            );
            break_lines_at_text(&mut panel);
        }
        ChartType::Line | ChartType::Area if !block.stack => {
            place::value_labels(ctx, facet, &mut panel, place::ValueLabels::Series)
        }
        _ => {}
    }
    // Names inside the plot come last, so they keep clear of every value and annotation.
    if inside_names {
        let texts: Vec<Rect> = panel
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text(t) if t.bounds.overlaps(&plot, 0.0) => Some(t.bounds),
                _ => None,
            })
            .collect();
        let lines = series_lines(ctx, p, members, &xl, &fr.y, plot, false);
        match place::inside_name_places(ctx, plot, &lines, &texts, kind == ChartType::Area) {
            Some(places) => place::inside_names(ctx, &places, &mut panel),
            None => *named = false,
        }
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

/// Where a vertical bar chart makes room above its tallest bar for that bar's value.
enum BarRoom {
    /// Pixels the value axis keeps above its largest value: the axis ends a step higher.
    Axis(f64),
    /// Pixels the plot still lacks above the value's place, under a maximum the author fixed:
    /// the axis keeps the author's ends and the plot starts lower.
    Plot(f64),
}

/// The room a vertical bar chart keeps above its largest value `hi`, so that the value of its
/// tallest bar has its place above the bar (FEA-CH-28): the axis ends a step higher when its
/// last tick would crowd that place, or, when the author fixed the maximum, the plot starts
/// lower. `None` when the bars hold their values inside, are stacked, the author fixed only the
/// minimum or switched values off, or the place is already clear: under the y `title` when that
/// lies over it, else under `top`, the frame's upper edge.
#[allow(clippy::too_many_arguments)]
fn bar_value_room(
    ctx: &Ctx,
    p: &Parsed,
    members: &Members,
    hi: f64,
    xl: &XLayout,
    y: &ValueAxis,
    plot: Rect,
    title: Option<Rect>,
    top: f64,
) -> Option<BarRoom> {
    let block = ctx.block;
    let fixed_max = block.y.max.is_some();
    if block.kind != ChartType::Bar
        || block.stack
        || !block.auto_annotations
        || (block.y.min.is_some() && !fixed_max)
        || hi.is_nan()
        || hi <= 0.0
    {
        return None;
    }
    // Panels that share a scale decide alike: by the chart's tallest bar, wherever it stands.
    let shared = block.encoding.facet.is_some() && !block.style.free_scales;
    let shown = |o: &&Obs| p.self_colour || !ctx.is_hidden(o.series);
    let taller = |a: &(f64, &Obs), b: &(f64, &Obs)| a.0.total_cmp(&b.0);
    let (value, tallest) = if shared {
        p.obs
            .iter()
            .filter(shown)
            .filter_map(|o| Some((o.y?, o)))
            .max_by(taller)?
    } else {
        members
            .obs
            .iter()
            .map(|i| &p.obs[*i])
            .filter(shown)
            .filter_map(|o| Some((o.y?, o)))
            .max_by(taller)?
    };
    let readout = block.readout_format(&block.y, ctx.locale);
    // The room is the printed value's, at the label's digits (FEA-CH-37).
    let printed = ctx
        .value_label(value, &readout, block.label_digits(&block.y))
        .unwrap_or_else(|| numbers::value(value, &readout));
    let [w, above, clear] = place::bar_value_above(ctx, &printed);
    let visible: Vec<usize> = (0..ctx.series.len()).filter(|s| !ctx.is_hidden(*s)).collect();
    let groups = if p.self_colour { 1 } else { visible.len().max(1) };
    let inner = xl.band * 0.72;
    let thickness = (inner / groups as f64).min(ctx.style.bar_cap(super::BAR_CAP));
    let bar_top = y.map(hi, plot.bottom(), plot.y);
    if w + 8.0 <= (thickness - 1.0).max(1.0) && above + 5.0 <= plot.bottom() - bar_top {
        return None;
    }
    let (ceiling, gap) = match title {
        Some(title) if shared => (title.bottom(), clear),
        Some(title) => {
            // The value's box, centred on its bar and slid into the plot as its place will be.
            let centre = xl.map(tallest.x);
            let left = if groups == 1 {
                centre - thickness / 2.0
            } else {
                let slot = visible.iter().position(|s| *s == tallest.series).unwrap_or(0);
                centre - groups as f64 * thickness / 2.0 + slot as f64 * thickness
            } + thickness / 2.0
                - w / 2.0;
            let left = left.clamp(plot.x, (plot.right() - w).max(plot.x));
            if left - clear < title.right() && title.x < left + w + clear {
                (title.bottom(), clear)
            } else {
                (top, 0.0)
            }
        }
        None => (top, 0.0),
    };
    let place = bar_top - above - gap;
    (place < ceiling).then(|| {
        if fixed_max {
            // The plot's bottom stays, so lowering its top by `d` lowers the bar's top by its
            // share of the plot's height times `d`: the plot moves by the shortfall over that share.
            let share = ((plot.bottom() - bar_top) / plot.h).clamp(0.1, 1.0);
            BarRoom::Plot((ceiling - place) / share + 0.05)
        } else {
            BarRoom::Axis(above + gap - (plot.y - ceiling))
        }
    })
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

/// Each series' line in pixels, for the search of its name's place inside the plot: the
/// observations a zoom window draws, in x order, hidden series included (`all_shown` reads every
/// series as shown). A stacked area's line is the top of its part of the stack; a hidden part's
/// line runs where it would stand on the parts shown below it.
fn series_lines(
    ctx: &Ctx,
    p: &Parsed,
    members: &Members,
    xl: &XLayout,
    y: &ValueAxis,
    plot: Rect,
    all_shown: bool,
) -> Vec<place::NameLine> {
    let window = xl.scale.as_ref().map(|s| s.domain);
    let stacked = ctx.block.stack && ctx.block.kind == ChartType::Area;
    let key = |x: f64| (x * 1000.0).round() as i64;
    let mut bases: [std::collections::HashMap<i64, f64>; 2] = Default::default();
    (0..ctx.series.len())
        .map(|s| {
            let pts = &members.series[s];
            let hidden = !all_shown && ctx.is_hidden(s);
            let points = pts[line_span(p, pts, window)]
                .iter()
                .filter_map(|&k| {
                    let o = &p.obs[k];
                    let v = o.y?;
                    let top = if stacked {
                        let base = bases[usize::from(v < 0.0)].entry(key(o.x)).or_default();
                        let top = *base + v;
                        if !hidden {
                            *base = top;
                        }
                        top
                    } else {
                        v
                    };
                    Some([xl.map(o.x), y.map(top.clamp(y.lo, y.hi), plot.bottom(), plot.y)])
                })
                .collect();
            place::NameLine {
                series: s,
                points,
                hidden,
                // Hover dots are drawn only where the chart is not stacked (`observations`).
                dots: !ctx.block.stack,
            }
        })
        .collect()
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

/// First area boundaries in data units: the order an area's series are recorded in (TYP-36).
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
    inside_names: bool,
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
        // neighbours, and a zoomed layout reads only the points it draws (PRF-19).
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
            // Markers tell one line from another where colour alone cannot; a single line has
            // none to be told from, and its markers would read as marked observations (chart
            // design, 16 Nov 2026: Wheat and wages in Hand-drawn).
            if let Some(shape) = ctx.series[s].shape.filter(|_| ctx.series.len() > 1) {
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
    // series' line and overlaps blend in the series' own hues. Within each, muted series paint
    // first, so no quiet line crosses over a highlighted one (TYP-78).
    items[painted_from..].sort_by_key(|i| {
        let muted = i.series().is_some_and(|s| ctx.mutes_series(s));
        (!matches!(i, Item::Area { role: Role::Mark, .. }), !muted)
    });
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
    // A shown series the author's highlight mutes is not named at its end (TYP-78).
    ends.retain(|(s, _, _)| !ctx.mutes_series(*s) || ctx.is_hidden(*s));
    // Data records do not depend on whether the style has room for direct names.
    let end_order = place::end_name_order(ctx, panel_of(obs), &ends);
    if end_names && !inside_names {
        place::end_names(ctx, plot, &ends, &end_order, items, taken);
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
                label: None,
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
    let thickness = (inner / groups as f64).min(ctx.style.bar_cap(super::BAR_CAP));
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
            // The group stands centred on its category, also when the bars' cap leaves part of
            // the band empty: a group starting at the band's inner edge sat beside its label.
            let left = if groups == 1 {
                center - thickness / 2.0
            } else {
                center - groups as f64 * thickness / 2.0 + slot as f64 * thickness
            };
            let Some(o) = o else {
                // Impossible combinations reserve no slot when bars colour themselves.
                continue;
            };
            let own = if p.self_colour {
                ctx.style.palette[ctx.series.get(cat).map_or(cat, |s| s.slot) % ctx.style.palette.len()]
            } else {
                ctx.series_color(s)
            };
            let color = Style::rgba(ctx.mark_color(o.row, series_index, own));
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
                    label: None,
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
    let label = v.and_then(|v| ctx.value_label(v, f, ctx.block.label_digits(&ctx.block.y)));
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
                label,
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
    let thickness = (inner / groups as f64).min(ctx.style.bar_cap(super::BAR_CAP_ACROSS));
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
                center - groups as f64 * thickness / 2.0 + slot as f64 * thickness
            };
            let own = if p.self_colour {
                ctx.style.palette[ctx.series.get(cat).map_or(cat, |s| s.slot) % ctx.style.palette.len()]
            } else {
                ctx.series_color(s)
            };
            let color = Style::rgba(ctx.mark_color(o.row, series_index, own));
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

/// Room a gridline or zero line leaves round text it would cross, past the text's box.
const TEXT_GAP: f64 = 2.0;

/// Each straight gridline or zero line of a bar chart breaks where it would run through text,
/// `TEXT_GAP` past the text's box, and runs on everywhere else: through the names set above
/// horizontal bars under 480 px, and through values set across a gridline at any width. No
/// knockout box is painted, so a tinted plot stays true (FEA-CH-26, FEA-CH-28).
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
        let [Cmd::M { p: a }, Cmd::L { p: b }] = path.cmds[..] else {
            continue;
        };
        // The axis the line runs along, 1 for a vertical line and 0 for a horizontal one, and
        // where it stands on the other.
        let along = match (a[0] == b[0], a[1] == b[1]) {
            (true, false) => 1,
            (false, true) => 0,
            _ => continue,
        };
        let (at, start, end) = (a[1 - along], a[along].min(b[along]), a[along].max(b[along]));
        let point = |v: f64| if along == 1 { [at, v] } else { [v, at] };
        // A text box's extent along the line and across it.
        let extent = |r: &Rect, axis: usize| if axis == 1 { [r.y, r.bottom()] } else { [r.x, r.right()] };
        let reach = TEXT_GAP + stroke.width / 2.0;
        let mut gaps: Vec<[f64; 2]> = boxes
            .iter()
            .filter_map(|r| {
                let ([lo, hi], [low, high]) = (extent(r, 1 - along), extent(r, along));
                (at >= lo - reach && at <= hi + reach && high + TEXT_GAP > start && low - TEXT_GAP < end)
                    .then_some([low - TEXT_GAP, high + TEXT_GAP])
            })
            .collect();
        if gaps.is_empty() {
            continue;
        }
        gaps.sort_by(|p, q| p[0].total_cmp(&q[0]));
        let mut broken = Path::new();
        let mut from = start;
        for [gap_start, gap_end] in gaps.into_iter().chain([[end, end]]) {
            let to = gap_start.min(end);
            // A stub shorter than a pixel reads as a speck, not a line.
            if to - from >= 1.0 {
                broken.move_to(point(from));
                broken.line_to(point(to));
            }
            from = from.max(gap_end);
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
    y_window: Option<[f64; 2]>,
    items: &mut Vec<Item>,
    observations: &mut Vec<Observation>,
    taken: &mut Vec<Rect>,
) -> Option<Vec<(usize, [f64; 2], f64)>> {
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
    let quiet = Style::rgba(ctx.style.ink_faint);
    let painted_from = items.len();
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
        if let Some(w) = y_window
            && (v < w[0] || v > w[1])
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
                    if ctx.mutes_mark(o.row, o.series) {
                        quiet
                    } else {
                        colors[o.series]
                    },
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
                let muted = point.row.is_some_and(|row| ctx.mutes_mark(row, s));
                items.push(point_item(
                    point.at,
                    radius,
                    s,
                    observation,
                    if muted { quiet } else { colors[s] },
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
    // Muted points paint first, so none covers a highlighted one (TYP-78).
    if ctx.emphasis.is_some() {
        let muted = |i: &Item| match i {
            Item::Point {
                obs: Some(k),
                series: Some(s),
                ..
            } => observations[*k].row.is_some_and(|row| ctx.mutes_mark(row, *s)),
            _ => false,
        };
        items[painted_from..].sort_by_key(|i| !muted(i));
    }
    if !labelled {
        return None;
    }
    // On an annotated chart the points to name are returned: the caller places the author's
    // annotations first, then the names.
    if !block.annotations.is_empty() {
        return Some(anchors);
    }
    place::point_labels(ctx, plot, &anchors, items, taken, &place::Drawn::default());
    None
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
    use crate::geometry::{Geometry, Item, Rect, Role, Text};
    use crate::layout::{ChartTheme, Options, layout};
    use serde_json::json;

    fn opts(width: f64) -> Options {
        Options {
            width,
            ..Options::default()
        }
    }

    /// The series names among a chart's labels, with their boxes.
    fn series_names(g: &Geometry) -> Vec<(String, Rect)> {
        g.items()
            .filter_map(|i| match i {
                Item::Text(t)
                    if t.role == Role::Label
                        && t.face == crate::geometry::Face::Data
                        && t.series.is_some_and(|s| g.series[s].name == t.text) =>
                {
                    Some((t.text.clone(), t.bounds))
                }
                _ => None,
            })
            .collect()
    }

    /// Whether a drawn line of another series passes within 3 px of a name's box.
    fn a_line_crosses_a_name(g: &Geometry) -> bool {
        use crate::layout::place::segment_touches_rect;
        series_names(g).iter().any(|(name, bounds)| {
            g.items().any(|i| match i {
                Item::Line {
                    path,
                    role: Role::Mark,
                    series: Some(s),
                    ..
                } if g.series[*s].name != *name => path.flatten(0.5).iter().any(|run| {
                    run.windows(2)
                        .any(|w| segment_touches_rect(w[0], w[1], &bounds.inflate(3.0)))
                }),
                _ => false,
            })
        })
    }

    fn two_crossing_lines() -> Vec<serde_json::Value> {
        (1..=12)
            .flat_map(|m| {
                [
                    json!({"m": format!("2024-{m:02}"), "v": m * 10, "s": "North"}),
                    json!({"m": format!("2024-{m:02}"), "v": 130 - m * 10, "s": "South"}),
                ]
            })
            .collect()
    }

    #[test]
    fn lines_with_room_are_named_inside_the_plot_which_takes_the_width() {
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "color": "s"}}),
        )
        .unwrap();
        let rows = two_crossing_lines();
        for width in [318.0, 390.0, 640.0, 1200.0] {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            assert!(g.failure.is_none());
            assert!(g.legend.is_none(), "{width}: the names stand in the chart");
            let plot = g.panels[0].plot;
            let names = series_names(&g);
            let mut found: Vec<&str> = names.iter().map(|(name, _)| name.as_str()).collect();
            found.sort_unstable();
            assert_eq!(found, ["North", "South"], "{width}");
            for (name, bounds) in &names {
                assert!(bounds.inside(&plot, 0.0), "{width}: {name} stands inside the plot");
            }
            assert!(!a_line_crosses_a_name(&g), "{width}: no other line runs through a name");
            // No column of names beside the plot: only the room of the hover dot stays.
            let right = g.width - plot.right();
            assert!((9.0..12.0).contains(&right), "{width}: {right} px right of the plot");
            assert_eq!(g.panels[0].observations.len(), 12, "one readout per month");
            assert_eq!(g.panels[0].observations[0].readout.lines.len(), 2);
        }
        let g = layout(&block, &rows, &ChartTheme::default(), &opts(640.0));
        assert!(
            (g.width / g.height - 2.0).abs() < 0.3,
            "calendar lines keep 2:1, {} x {}",
            g.width,
            g.height
        );
    }

    #[test]
    fn lines_on_top_of_each_other_keep_their_names_at_the_line_ends() {
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "color": "s"}}),
        )
        .unwrap();
        // Three lines a hair apart all the way: no name has a place of its own between them.
        let rows: Vec<_> = (1..=12)
            .flat_map(|m| {
                ["North", "South", "West"]
                    .into_iter()
                    .enumerate()
                    .map(move |(k, s)| json!({"m": format!("2024-{m:02}"), "v": m * 10 + k, "s": s}))
            })
            .collect();
        for width in [390.0, 640.0, 1200.0] {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            assert!(g.failure.is_none());
            assert!(g.legend.is_none(), "{width}");
            let plot = g.panels[0].plot;
            let names = series_names(&g);
            assert_eq!(names.len(), 3, "{width}");
            for (name, bounds) in &names {
                assert!(
                    bounds.x > plot.right(),
                    "{width}: {name} stands at its line's end, right of the plot"
                );
            }
        }
    }

    #[test]
    fn a_name_inside_the_plot_keeps_clear_of_every_value_and_gridline() {
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "color": "s"}}),
        )
        .unwrap();
        let rows = two_crossing_lines();
        let mut inside = 0;
        for width in (300..=1200).step_by(20) {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(f64::from(width)));
            assert!(g.failure.is_none());
            let plot = g.panels[0].plot;
            let names = series_names(&g);
            if !names.iter().all(|(_, bounds)| bounds.inside(&plot, 0.0)) {
                continue;
            }
            inside += 1;
            for (name, bounds) in &names {
                // A value never stands on a name, nor beside it closer than 8 px.
                let beside = Rect::new(bounds.x - 8.0, bounds.y, bounds.w + 16.0, bounds.h);
                for item in g.items() {
                    match item {
                        Item::Text(t) if t.role == Role::Label && !names.iter().any(|(n, _)| *n == t.text) => {
                            assert!(
                                !t.bounds.overlaps(&beside, 0.0),
                                "{width}: {} stands against {name}",
                                t.text
                            );
                        }
                        Item::Line {
                            path, role: Role::Grid, ..
                        } => {
                            let crossed = path.flatten(0.5).iter().any(|run| {
                                run.windows(2)
                                    .any(|w| crate::layout::place::segment_touches_rect(w[0], w[1], bounds))
                            });
                            assert!(!crossed, "{width}: a gridline runs through {name}");
                        }
                        _ => {}
                    }
                }
            }
        }
        assert!(
            inside > 20,
            "most widths name the two lines inside the plot, {inside} did"
        );
    }

    #[test]
    fn names_that_cannot_stand_inside_go_to_the_legend_when_their_column_is_too_wide() {
        // Three lines a hair apart with long names: the column would take a third of the plot.
        let rows: Vec<_> = (1..=12)
            .flat_map(|m| {
                ["Hacker News comments", "GitHub cloud-agent PRs", "npm CLI downloads"]
                    .into_iter()
                    .enumerate()
                    .map(move |(k, s)| json!({"m": format!("2024-{m:02}"), "v": m * 10 + k, "s": s}))
            })
            .collect();
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "color": "s"}}),
        )
        .unwrap();
        let g = layout(&block, &rows, &ChartTheme::default(), &opts(632.0));
        assert!(g.failure.is_none());
        assert!(g.legend.is_some(), "the names stand in the legend row");
        assert!(series_names(&g).is_empty());
        assert!(g.width - g.panels[0].plot.right() < 12.0, "the plot takes the width");
    }

    #[test]
    fn an_areas_name_stands_inside_its_own_fill_and_never_in_the_axis_margin() {
        let rows: Vec<_> = ["Mon", "Tue", "Wed", "Thu"]
            .iter()
            .enumerate()
            .flat_map(|(i, day)| {
                [
                    json!({"day": day, "tier": "free", "users": 820 + 30 * i}),
                    json!({"day": day, "tier": "paid", "users": 240 + 20 * i}),
                ]
            })
            .collect();
        let block = ChartBlock::from_value(
            &json!({"type": "area", "title": "T", "encoding": {"x": "day", "y": "users", "color": "tier"}}),
        )
        .unwrap();
        for width in [318.0, 390.0, 640.0, 960.0] {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            assert!(g.failure.is_none());
            let plot = g.panels[0].plot;
            let names = series_names(&g);
            assert!(
                names.iter().all(|(_, bounds)| bounds.inside(&plot, 0.0)),
                "{width}: no name stands in the value axis' margin"
            );
            assert!(
                g.legend.is_some() != (names.len() == 2),
                "{width}: names inside, or the legend"
            );
            if width >= 640.0 {
                assert_eq!(names.len(), 2, "{width}: both names have room inside");
            }
            // Inside its own fill: below its own line, which starts at 820 (free) or 240 (paid).
            for (name, bounds) in &names {
                let own = g
                    .items()
                    .find_map(|i| match i {
                        Item::Line {
                            path,
                            role: Role::Mark,
                            series: Some(s),
                            ..
                        } if g.series[*s].name == *name => path.flatten(0.5).into_iter().next(),
                        _ => None,
                    })
                    .unwrap();
                let above_name = own
                    .iter()
                    .filter(|p| p[0] >= bounds.x && p[0] <= bounds.right())
                    .all(|p| p[1] < bounds.y);
                assert!(above_name, "{width}: {name} stands below its own line");
            }
        }
    }

    #[test]
    fn a_hidden_lines_name_stays_inside_the_plot_as_its_way_back() {
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "color": "s"}}),
        )
        .unwrap();
        let rows = two_crossing_lines();
        let hidden = Options {
            width: 640.0,
            hidden: vec!["South".into()],
            ..Options::default()
        };
        let g = layout(&block, &rows, &ChartTheme::default(), &hidden);
        assert!(g.failure.is_none());
        assert!(g.legend.is_none());
        let plot = g.panels[0].plot;
        let names = series_names(&g);
        assert_eq!(names.len(), 2, "the hidden series keeps its name");
        assert!(names.iter().all(|(_, bounds)| bounds.inside(&plot, 0.0)));
        assert!(!names[0].1.inflate(2.0).overlaps(&names[1].1, 0.0));
    }

    /// Hiding a series lets the value axis follow the rest, so a flat line may turn steep. The
    /// names still stay inside the plot, and the hidden one keeps its place where that is free.
    #[test]
    fn hiding_a_series_keeps_every_name_inside_the_plot() {
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "color": "s"}}),
        )
        .unwrap();
        let rows: Vec<serde_json::Value> = (0..4)
            .flat_map(|i| {
                [("Long series name one", 1), ("Two", 3)]
                    .map(|(s, k)| json!({"m": format!("2024-{:02}", i + 1), "v": i * k + 2, "s": s}))
            })
            .collect();
        for width in [358.0, 640.0, 1100.0] {
            let all = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            assert!(all.legend.is_none(), "{width}: named inside with every series shown");
            for hide in ["Long series name one", "Two"] {
                let hidden = Options {
                    width,
                    hidden: vec![hide.into()],
                    ..Options::default()
                };
                let g = layout(&block, &rows, &ChartTheme::default(), &hidden);
                assert!(g.legend.is_none(), "{width}: hiding {hide} brings no legend");
                let plot = g.panels[0].plot;
                let names = series_names(&g);
                assert_eq!(names.len(), 2, "{width}: hiding {hide} keeps both names");
                assert!(
                    names.iter().all(|(_, bounds)| bounds.inside(&plot, 0.0)),
                    "{width}: hiding {hide} keeps the names inside the plot"
                );
                assert!(!a_line_crosses_a_name(&g), "{width}: hiding {hide}");
            }
        }
    }

    /// A hidden series' name stands where its data is: when the refitted axis leaves the data
    /// below the plot, against the plot's bottom edge at the right, and never on the line that
    /// stays (chart design, 16 Nov 2026: Mentions at 390 with Fieldwork hidden).
    #[test]
    fn a_hidden_name_stands_where_its_data_is() {
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "color": "s"}}),
        )
        .unwrap();
        let rows: Vec<serde_json::Value> = [("Northstar", [2210, 2190, 2060]), ("Fieldwork", [1120, 1340, 1580])]
            .iter()
            .flat_map(|(s, v)| {
                v.iter()
                    .enumerate()
                    .map(move |(i, v)| json!({"m": format!("2026-{:02}", i + 4), "v": v, "s": s}))
            })
            .collect();
        for width in [318.0, 632.0] {
            let hidden = Options {
                width,
                hidden: vec!["Fieldwork".into()],
                ..Options::default()
            };
            let g = layout(&block, &rows, &ChartTheme::default(), &hidden);
            assert!(g.legend.is_none(), "{width}");
            let plot = g.panels[0].plot;
            let names = series_names(&g);
            let field = names
                .iter()
                .find(|(n, _)| n == "Fieldwork")
                .expect("the hidden name stays")
                .1;
            assert!(field.inside(&plot, 0.0), "{width}: {field:?}");
            assert!(
                field.bottom() > plot.bottom() - plot.h / 4.0,
                "{width}: the hidden name stands at the bottom, on its data's side: {field:?} in {plot:?}"
            );
            assert!(field.center()[0] > plot.x + plot.w / 2.0, "{width}: at the right");
            assert!(!a_line_crosses_a_name(&g), "{width}: never on Northstar's line");
        }
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
    fn an_area_over_categories_runs_from_edge_to_edge_of_its_plot() {
        let rows: Vec<_> = ["Mon", "Tue", "Wed", "Thu"]
            .iter()
            .enumerate()
            .flat_map(|(i, day)| {
                [
                    json!({"day": day, "tier": "free", "users": 820 + 30 * i}),
                    json!({"day": day, "tier": "paid", "users": 240 + 20 * i}),
                ]
            })
            .collect();
        for width in [318.0, 640.0, 960.0] {
            let block = |kind: &str| {
                ChartBlock::from_value(
                    &json!({"type": kind, "title": "T", "encoding": {"x": "day", "y": "users", "color": "tier"}}),
                )
                .unwrap()
            };
            let g = layout(&block("area"), &rows, &ChartTheme::default(), &opts(width));
            assert!(g.failure.is_none());
            let panel = &g.panels[0];
            let xs: Vec<f64> = panel.observations.iter().map(|o| o.at[0]).collect();
            let (first, last) = (
                xs.iter().copied().fold(f64::INFINITY, f64::min),
                xs.iter().copied().fold(0.0, f64::max),
            );
            assert!(
                (first - panel.plot.x).abs() < 0.01,
                "{width}: the area starts at {first}, the plot at {}",
                panel.plot.x
            );
            assert!(
                (last - panel.plot.right()).abs() < 0.01,
                "{width}: the area ends at {last}, the plot at {}",
                panel.plot.right()
            );
            // Every category name stays, inside the figure.
            let names: Vec<&Text> = g
                .items()
                .filter_map(|i| match i {
                    Item::Text(t)
                        if t.role == Role::AxisLabel && ["Mon", "Tue", "Wed", "Thu"].contains(&t.text.as_str()) =>
                    {
                        Some(t)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(names.len(), 4, "{width}");
            assert!(
                names
                    .iter()
                    .all(|t| t.bounds.x >= 0.0 && t.bounds.right() <= g.width + 0.01),
                "{width}"
            );
            // Bars keep a band for each category.
            let bars = layout(&block("bar"), &rows, &ChartTheme::default(), &opts(width));
            let plot = bars.panels[0].plot;
            assert!(
                bars.panels[0]
                    .observations
                    .iter()
                    .all(|o| o.at[0] > plot.x + 8.0 && o.at[0] < plot.right() - 8.0)
            );
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
    fn a_line_or_an_area_over_names_zooms_and_bars_do_not() {
        let months = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let rows: Vec<_> = months
            .iter()
            .enumerate()
            .map(|(i, m)| json!({"m": m, "v": 10 + i * 3}))
            .collect();
        let block = |kind: &str| {
            ChartBlock::from_value(&json!({"type": kind, "title": "T", "encoding": {"x": "m", "y": "v"}})).unwrap()
        };
        let draw = |kind: &str, zoom: Option<[f64; 2]>| {
            let g = layout(
                &block(kind),
                &rows,
                &ChartTheme::default(),
                &Options { zoom, ..opts(640.0) },
            );
            assert!(g.failure.is_none());
            g
        };
        let ticks = |g: &crate::geometry::Geometry| -> Vec<String> {
            g.items()
                .filter_map(|i| match i {
                    Item::Text(t) if t.role == Role::AxisLabel && months.contains(&t.text.as_str()) => {
                        Some(t.text.clone())
                    }
                    _ => None,
                })
                .collect()
        };
        for kind in ["line", "area"] {
            let whole = draw(kind, None);
            let scale = whole.panels[0].x.clone().expect("named positions zoom");
            assert_eq!(scale.least, Some(1.0));
            // A line keeps half a band beyond its first and last name; an area runs edge to edge.
            let extent = if kind == "area" { [0.0, 11.0] } else { [-0.5, 11.5] };
            assert_eq!(scale.extent, extent);
            assert_eq!(scale.domain, scale.extent);
            assert_eq!(ticks(&whole), months);
            // A window drawn across the plot: its names are labelled and read out, the others
            // are not, and each stands where the window puts it.
            let window = [2.0, 5.0];
            let g = draw(kind, Some(window));
            let panel = &g.panels[0];
            assert_eq!(panel.x.as_ref().unwrap().domain, window);
            assert_eq!(panel.plot, whole.panels[0].plot, "the plot keeps its place");
            assert_eq!(ticks(&g), ["Mar", "Apr", "May", "Jun"]);
            let read: Vec<&str> = panel.observations.iter().map(|o| o.readout.heading.as_str()).collect();
            assert_eq!(read, ["Mar", "Apr", "May", "Jun"]);
            let per = panel.plot.w / (window[1] - window[0]);
            for (i, o) in panel.observations.iter().enumerate() {
                let at = panel.plot.x + (i as f64 + 2.0 - window[0]) * per;
                assert!((o.at[0] - at).abs() < 1e-6, "{kind} {i}: {} against {at}", o.at[0]);
            }
            // A window from name to name puts both on the plot's edges, line and area alike.
            assert!((panel.observations[0].at[0] - panel.plot.x).abs() < 1e-6);
            assert!((panel.observations[3].at[0] - panel.plot.right()).abs() < 1e-6);
            // A window narrower than from one name to the next widens to that; one beyond the
            // end stops at the end.
            let g = draw(kind, Some([4.9, 5.1]));
            assert_eq!(g.panels[0].x.as_ref().unwrap().domain, [4.5, 5.5]);
            let g = draw(kind, Some([10.0, 14.0]));
            assert_eq!(g.panels[0].x.as_ref().unwrap().domain, [extent[1] - 4.0, extent[1]]);
        }
        // Bars keep every category, with or without a window, and say they do not zoom.
        let bars = draw("bar", Some([1.5, 5.5]));
        assert!(bars.panels[0].x.is_none());
        assert_eq!(bars, draw("bar", None));
        // A line or an area over two names does not zoom (Chart design, 16 November).
        let two: Vec<_> = rows[..2].to_vec();
        for kind in ["line", "area"] {
            let g = layout(
                &block(kind),
                &two,
                &ChartTheme::default(),
                &Options {
                    zoom: Some([0.0, 0.5]),
                    ..opts(640.0)
                },
            );
            assert!(g.panels[0].x.is_none(), "{kind} over two names");
        }
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

    /// A monthly series that rises through a yearly cycle, as the Keeling curve does: its highest
    /// value is a few months before its last, so the two labels want the same corner.
    fn rising_cycle() -> Vec<serde_json::Value> {
        (0..822)
            .map(|i| {
                let t = i as f64;
                let y = 315.0 + 0.137 * t + 3.2 * (std::f64::consts::TAU * (t - 1.0) / 12.0).sin();
                json!({"x": i, "y": (y * 100.0).round() / 100.0})
            })
            .collect()
    }

    #[test]
    fn a_lines_last_value_stays_when_it_and_the_highest_cannot_both_stand() {
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "x", "y": "y"}, "axes": {"y": {"unit": "ppm", "digits": 5}}}),
        )
        .unwrap();
        // The line ends five points past its highest, as the Keeling curve ends in autumn.
        let rows: Vec<_> = rising_cycle()
            .into_iter()
            .chain((822..826).map(|i| {
                let t = f64::from(i);
                let y = 315.0 + 0.137 * t + 3.2 * (std::f64::consts::TAU * (t - 1.0) / 12.0).sin();
                json!({"x": i, "y": (y * 100.0).round() / 100.0})
            }))
            .collect();
        let mut both = 0;
        for width in (240..=720).step_by(10) {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(f64::from(width)));
            assert!(g.failure.is_none());
            let shown = |row: usize| {
                g.items().any(|i| matches!(i, Item::Text(t) if t.role == Role::Label && t.mark.as_deref() == Some(format!("0/-/{row}").as_str())))
            };
            assert!(
                shown(825) || !shown(820),
                "{width}: the highest value shows without the last"
            );
            both += usize::from(shown(825) && shown(820));
        }
        assert!(both > 0, "a wide plot holds both");
    }

    /// NASA's annual temperature index ends on its second-highest year after its highest (1.28 in
    /// 2024, 1.18 in 2025). The two values stand at the line's end on leaders: the higher stands
    /// above, the leaders never cross, and the last value shows at every width (chart design,
    /// 16 Nov 2026, A warmer world at 1440).
    #[test]
    fn values_at_a_lines_end_keep_their_order_and_their_leaders_never_cross() {
        use crate::layout::place::segments_cross;
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "year", "y": "anomaly_c"},
            "axes": {"x": {"format": "year"}, "y": {"label": "Temperature anomaly", "unit": "°C"}}}),
        )
        .unwrap();
        let rows: Vec<serde_json::Value> = include_str!("../../tests/fixtures/global-temperature.csv")
            .lines()
            .skip(1)
            .filter_map(|l| l.split_once(','))
            .map(|(y, v)| json!({"year": y.parse::<i64>().unwrap(), "anomaly_c": v.parse::<f64>().unwrap()}))
            .collect();
        let number = |t: &str| -> f64 {
            t.trim_end_matches("°C")
                .replace('\u{a0}', "")
                .replace('\u{2212}', "-")
                .trim()
                .parse()
                .unwrap()
        };
        for width in (240..=1440).step_by(8) {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(f64::from(width)));
            assert!(g.failure.is_none());
            let values: Vec<_> = g
                .items()
                .filter_map(|i| match i {
                    Item::Text(t) if t.role == Role::Label && t.mark.is_some() => Some((number(&t.text), t.bounds)),
                    _ => None,
                })
                .collect();
            assert!(
                values.iter().any(|(v, _)| (*v - 1.18).abs() < 1e-9),
                "{width}: the last value shows: {values:?}"
            );
            for (i, (a, ra)) in values.iter().enumerate() {
                for (b, rb) in &values[i + 1..] {
                    let stacked = ra.x < rb.right() && rb.x < ra.right();
                    if stacked && a != b {
                        assert_eq!(
                            a > b,
                            ra.center()[1] < rb.center()[1],
                            "{width}: of {a} and {b}, one above the other, the higher stands above: {ra:?} {rb:?}"
                        );
                    }
                }
            }
            let leaders: Vec<_> = g
                .items()
                .filter_map(|i| match i {
                    Item::Line {
                        path,
                        role: Role::Leader,
                        ..
                    } => path.flatten(0.5).into_iter().next().map(|p| (p[0], p[p.len() - 1])),
                    _ => None,
                })
                .collect();
            for (i, (a, b)) in leaders.iter().enumerate() {
                for (c, d) in &leaders[i + 1..] {
                    assert!(
                        !segments_cross(*a, *b, *c, *d),
                        "{width}: two leaders cross: {leaders:?}"
                    );
                }
            }
        }
    }

    /// An area over categories runs edge to edge, so its end values have no room beside the
    /// plot. Each stands above its own line, on the page or on the tint of an area above it, at
    /// 4.5:1 there (chart design, 16 Nov 2026: the docs' "Daily active users by tier").
    #[test]
    fn an_areas_end_values_stand_above_its_own_line() {
        let block = ChartBlock::from_value(&json!({"type": "area", "title": "Daily active users by tier",
            "encoding": {"x": "day", "y": "users", "color": "tier"}}))
        .unwrap();
        let rows: Vec<_> = ["Mon", "Tue", "Wed", "Thu"]
            .iter()
            .enumerate()
            .flat_map(|(i, d)| {
                [
                    json!({"day": d, "tier": "free", "users": ([820, 910, 870, 940][i])}),
                    json!({"day": d, "tier": "paid", "users": ([240, 265, 290, 305][i])}),
                ]
            })
            .collect();
        for width in [318.0, 632.0] {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            assert!(g.failure.is_none());
            let panel = &g.panels[0];
            let values: Vec<_> = panel
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Text(t) if t.role == Role::Label && t.face == crate::geometry::Face::Figures => Some(t),
                    _ => None,
                })
                .collect();
            let texts: Vec<_> = values.iter().map(|t| t.text.as_str()).collect();
            // At 318 px the 940 lies 9 px under the plot's top: no room above its line, so the
            // last values are left out and recorded. At 632 px they stand.
            for wanted in ["940", "305"] {
                assert!(
                    texts.contains(&wanted)
                        || (width < 400.0
                            && g.dropped
                                .iter()
                                .any(|d| d.text == wanted && d.kind.as_deref() == Some("value"))),
                    "{width}: the last value {wanted} shows or is recorded: {texts:?} {:?}",
                    g.dropped
                );
            }
            // A value standing across a gridline breaks it.
            for item in &panel.items {
                if let Item::Line {
                    path, role: Role::Grid, ..
                } = item
                {
                    for t in &values {
                        assert!(
                            !path
                                .flatten(0.5)
                                .iter()
                                .flat_map(|ring| ring.windows(2))
                                .any(|e| crate::layout::place::segment_touches_rect(e[0], e[1], &t.bounds)),
                            "{width}: a gridline runs through the value {}",
                            t.text
                        );
                    }
                }
            }
            for t in &values {
                // The value's own point: its series' observation nearest across.
                let x = t.bounds.center()[0];
                let point = panel
                    .observations
                    .iter()
                    .flat_map(|o| &o.dots)
                    .filter(|d| Some(d.series) == t.series)
                    .map(|d| d.at)
                    .min_by(|a, b| (a[0] - x).abs().total_cmp(&(b[0] - x).abs()))
                    .expect("a value has its series' points");
                assert!(
                    t.bounds.bottom() <= point[1],
                    "{width}: {} stands above its own line: {:?} over {point:?}",
                    t.text,
                    t.bounds
                );
            }
        }
    }

    /// A style with decals (Hand-drawn) marks lines with shapes so two lines can be told apart;
    /// a single line has nothing to be told from and draws none, since its markers would read as
    /// marked observations. Two lines keep their markers.
    #[test]
    fn a_single_line_draws_no_decal_markers() {
        let theme = ChartTheme {
            decals: true,
            ..ChartTheme::default()
        };
        let markers = |g: &crate::geometry::Geometry| {
            g.items()
                .filter(|i| matches!(i, Item::Point { role: Role::Mark, .. }))
                .count()
        };
        let one =
            ChartBlock::from_value(&json!({"type": "line", "title": "T", "encoding": {"x": "x", "y": "y"}})).unwrap();
        let rows: Vec<_> = (0..53).map(|x| json!({"x": x, "y": 40 + (x * 7) % 23})).collect();
        let g = layout(&one, &rows, &theme, &opts(640.0));
        assert_eq!(markers(&g), 0, "one line, no markers");
        let two = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "x", "y": "y", "color": "s"}}),
        )
        .unwrap();
        let rows: Vec<_> = (0..53)
            .flat_map(|x| {
                [
                    json!({"x": x, "y": 40 + (x * 7) % 23, "s": "a"}),
                    json!({"x": x, "y": 90 + (x * 5) % 17, "s": "b"}),
                ]
            })
            .collect();
        let g = layout(&two, &rows, &theme, &opts(640.0));
        assert!(markers(&g) > 0, "two lines keep their markers");
    }

    #[test]
    fn a_value_leader_never_crosses_another_value_and_values_keep_apart() {
        use crate::geometry::Rect;
        use crate::layout::place::segment_touches_rect;
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "x", "y": "y"}, "axes": {"y": {"unit": "ppm", "digits": 5}}}),
        )
        .unwrap();
        let rows = rising_cycle();
        for width in (240..=720).step_by(10) {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(f64::from(width)));
            assert!(g.failure.is_none());
            let values: Vec<_> = g
                .items()
                .filter_map(|i| match i {
                    Item::Text(t) if t.role == Role::Label && t.mark.is_some() => Some((t.text.clone(), t.bounds)),
                    _ => None,
                })
                .collect();
            let leaders: Vec<_> = g
                .items()
                .filter_map(|i| match i {
                    Item::Line {
                        path,
                        role: Role::Leader,
                        ..
                    } => path.flatten(0.5).into_iter().next(),
                    _ => None,
                })
                .collect();
            for (i, (a, ra)) in values.iter().enumerate() {
                for (b, rb) in &values[i + 1..] {
                    assert!(!ra.overlaps(rb, 0.0), "{width}: {a} and {b} overlap: {ra:?} {rb:?}");
                    // Side by side, two values keep a character's room, or they read as one.
                    let beside = Rect::new(ra.x - 7.5, ra.y, ra.w + 15.0, ra.h);
                    assert!(
                        !beside.overlaps(rb, 0.0),
                        "{width}: {a} and {b} run together: {ra:?} {rb:?}"
                    );
                }
            }
            for leader in &leaders {
                let (from, to) = (leader[0], leader[leader.len() - 1]);
                // A leader ends 2 px outside its own label; any label it touches is another's.
                let crossed: Vec<_> = values
                    .iter()
                    .filter(|(_, r)| segment_touches_rect(from, to, r))
                    .map(|(t, _)| t)
                    .collect();
                assert!(
                    crossed.is_empty(),
                    "{width}: the leader {from:?} to {to:?} crosses {crossed:?}; values {values:?}"
                );
            }
        }
    }

    #[test]
    fn a_point_name_on_a_leader_never_stands_behind_another_point() {
        use crate::geometry::Rect;
        use crate::layout::place::leader_spot;
        // A small point with a large neighbour 18 px to its left, as Congo beside India in 1964.
        let (at, size, plot) = ([300.0, 200.0], [96.0, 14.0], Rect::new(0.0, 0.0, 600.0, 400.0));
        let marks = [(0, at, 2.0), (1, [282.0, 200.0], 9.5)];
        // Names already placed take the whole right side, so the next place offered is on the left.
        let taken = [Rect::new(310.0, 100.0, 290.0, 200.0)];
        let spot = leader_spot(at, 2.0, size, plot, &taken, (&marks, 0), &|_| false).expect("a place on a leader");
        let near = [at[0].clamp(spot.x, spot.right()), at[1].clamp(spot.y, spot.bottom())];
        // The neighbour is not on the way from the point to its name.
        let (dx, dy) = (near[0] - at[0], near[1] - at[1]);
        let along = (((282.0 - at[0]) * dx + (200.0 - at[1]) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        let gap = (282.0 - at[0] - along * dx).hypot(200.0 - at[1] - along * dy);
        assert!(
            gap >= 9.5,
            "the name at {spot:?} has the neighbour between it and its point"
        );
        assert!(!spot.overlaps(&taken[0], 0.0));

        // A leader crosses no name already placed: with a name right under the point, the next
        // name does not go below it on a leader through that name.
        let under = [Rect::new(252.0, 216.0, 96.0, 14.0)];
        let marks = [(0, at, 2.0)];
        let spot = leader_spot(at, 2.0, [40.0, 14.0], plot, &under, (&marks, 0), &|_| false).expect("a place");
        let end = crate::layout::place::leader_end(at, spot).expect("a leader");
        assert!(
            !crate::layout::place::segment_touches_rect(at, end, &under[0]),
            "the leader to {spot:?} crosses the name under the point"
        );
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
    fn a_group_of_bars_stands_centred_on_its_category_when_their_width_is_capped() {
        // Two categories across a wide plot: each band could hold far wider bars than the cap.
        for horizontal in [false, true] {
            let mut raw = json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v", "color": "s"}});
            if horizontal {
                raw["style"] = json!({"orientation": "horizontal"});
            }
            let block = ChartBlock::from_value(&raw).unwrap();
            let rows = vec![
                json!({"c": "Aug", "s": "One", "v": 60}),
                json!({"c": "Aug", "s": "Two", "v": 50}),
                json!({"c": "Sep", "s": "One", "v": 80}),
                json!({"c": "Sep", "s": "Two", "v": 50}),
            ];
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(1100.0));
            let panel = &g.panels[0];
            let bars: Vec<Rect> = panel
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Rect {
                        rect, role: Role::Mark, ..
                    } => Some(*rect),
                    _ => None,
                })
                .collect();
            assert_eq!(bars.len(), 4, "{horizontal}");
            let across = |r: &Rect| if horizontal { (r.y, r.y + r.h) } else { (r.x, r.right()) };
            let (plot_from, plot_span) = if horizontal {
                (panel.plot.y, panel.plot.h)
            } else {
                (panel.plot.x, panel.plot.w)
            };
            let band = plot_span / 2.0;
            assert!(
                across(&bars[0]).1 - across(&bars[0]).0 < band * 0.36 - 1.0,
                "the cap applies"
            );
            for category in 0..2 {
                let start = plot_from + category as f64 * band;
                let mine: Vec<(f64, f64)> = bars
                    .iter()
                    .map(across)
                    .filter(|(a, b)| (a + b) / 2.0 >= start && (a + b) / 2.0 < start + band)
                    .collect();
                assert_eq!(mine.len(), 2, "{horizontal}: two bars in category {category}");
                let from = mine.iter().map(|m| m.0).fold(f64::INFINITY, f64::min);
                let to = mine.iter().map(|m| m.1).fold(f64::NEG_INFINITY, f64::max);
                let centre = start + band / 2.0;
                assert!(
                    ((from + to) / 2.0 - centre).abs() <= 1.0,
                    "{horizontal}: category {category}'s bars span {from:.1}..{to:.1}, its centre is {centre:.1}"
                );
            }
        }
    }

    #[test]
    fn on_a_slides_stage_bars_take_their_bands_share_without_the_cap() {
        let block = ChartBlock::from_value(
            &json!({"type": "bar", "title": "T", "stack": true, "encoding": {"x": "c", "y": "v", "color": "s"}}),
        )
        .unwrap();
        let rows: Vec<_> = ["Jul", "Aug", "Sep"]
            .iter()
            .flat_map(|c| [json!({"c": c, "s": "One", "v": 3}), json!({"c": c, "s": "Two", "v": 1})])
            .collect();
        let widest = |theme: &ChartTheme| {
            let g = layout(&block, &rows, theme, &opts(1600.0));
            let panel = &g.panels[0];
            let bars = panel.items.iter().filter_map(|i| match i {
                Item::Rect {
                    rect, role: Role::Mark, ..
                } => Some(rect.w),
                _ => None,
            });
            (bars.fold(0.0, f64::max), panel.plot.w / 3.0)
        };
        let (report, _) = widest(&ChartTheme::default());
        assert!(
            report <= crate::layout::BAR_CAP,
            "a report caps a column at 64 px: {report}"
        );
        let (stage, band) = widest(&ChartTheme {
            wide_bars: true,
            ..ChartTheme::default()
        });
        // 72 % of the band, less the pixel between neighbours.
        assert!(
            (stage - band * 0.72).abs() <= 1.5,
            "a slide's column takes its band's share: {stage} of {band}"
        );
    }

    #[test]
    fn on_a_slides_stage_values_are_set_larger_in_the_axis_compact_form() {
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"},
            "axes": {"y": {"format": "number_compact", "unit": "$"}}
        }))
        .unwrap();
        let rows = vec![json!({"c": "Aug", "v": 340_000}), json!({"c": "Sep", "v": 420_000})];
        let values = |theme: &ChartTheme| {
            let g = layout(&block, &rows, theme, &opts(1200.0));
            g.panels[0]
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Text(t) if t.role == Role::Label => Some((t.text.clone(), t.size)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let report = values(&ChartTheme::default());
        let stage = values(&ChartTheme {
            wide_bars: true,
            stage_values: true,
            ..ChartTheme::default()
        });
        let texts = |values: &[(String, f64)]| values.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();
        assert_eq!(texts(&report), ["$340 thousand", "$420 thousand"]);
        assert_eq!(texts(&stage), ["$340K", "$420K"], "the axis's form at the same digits");
        for ((_, size), (_, base)) in stage.iter().zip(&report) {
            assert!(
                (size - base * crate::layout::STAGE_VALUE_SCALE).abs() < 1e-9,
                "a stage's value is set larger: {size} against {base}"
            );
        }
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

    /// Two companies' shares per topic: grouped bars where one sibling often runs past the other.
    fn grouped_bars() -> (ChartBlock, Vec<serde_json::Value>) {
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "T", "encoding": {"x": "topic", "y": "share", "color": "company"},
            "axes": {"x": {"label": ""}, "y": {"label": "% of negative comments"}},
            "style": {"ratio": "16:9"}
        }))
        .unwrap();
        let rows = [
            ("Quality", 21.9, 17.6),
            ("Business", 16.9, 21.8),
            ("Safety", 20.3, 21.4),
            ("Hype", 13.5, 19.4),
            ("Reliability", 19.0, 10.7),
            ("Product", 14.3, 9.6),
            ("Pricing", 13.5, 8.7),
            ("Coding", 13.0, 4.0),
        ]
        .iter()
        .flat_map(|(topic, anthropic, openai)| {
            [
                json!({"topic": topic, "company": "Anthropic", "share": anthropic}),
                json!({"topic": topic, "company": "OpenAI", "share": openai}),
            ]
        })
        .collect();
        (block, rows)
    }

    #[test]
    fn a_grouped_value_slides_off_its_sibling_within_its_own_bar() {
        let (block, rows) = grouped_bars();
        for width in PHONE {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            let panel = &g.panels[0];
            let (bars, values) = bars_and_values(panel);
            assert_eq!(bars.len(), 16);
            let mut slid = 0;
            for (o, t) in &values {
                let bar = bars.iter().find(|(b, _)| b == o).unwrap().1;
                let middle = t.bounds.center()[1];
                assert!(
                    bar.y <= middle && middle <= bar.bottom(),
                    "{width}: '{}' is not level with its bar",
                    t.text
                );
                // A value inside its own bar is set off by the bar; one beside it keeps 1 px.
                let within = bar.x <= t.bounds.x && t.bounds.right() <= bar.right();
                for (_, other) in bars.iter().filter(|(b, _)| b != o && !within) {
                    assert!(
                        !t.bounds.inflate(0.99).overlaps(other, 0.0),
                        "{width}: '{}' comes within 1 px of another bar",
                        t.text
                    );
                }
                let centred = (middle - bar.center()[1]).abs() < 0.01;
                // Slid, it is flush with the bar's far edge, or past that by no more than the
                // sibling's own inside value needs: half of what a line box is taller than a bar.
                let room = ((t.bounds.h - bar.h) / 2.0).max(0.0) + 0.01;
                let (up, down) = (bar.bottom() - t.bounds.bottom(), t.bounds.y - bar.y);
                let flush = (-0.01..=room).contains(&up) || (-0.01..=room).contains(&down);
                assert!(centred || flush, "{width}: '{}' is neither centred nor flush", t.text);
                slid += usize::from(!centred);
            }
            assert!(slid > 0, "{width}: some value slides off its sibling: {:?}", g.dropped);
        }
    }

    #[test]
    fn a_value_with_no_room_past_its_end_sits_inside_the_bar_at_that_end() {
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "T", "encoding": {"x": "measure", "y": "share"},
            "axes": {"x": {"label": ""}, "y": {"label": "Anthropic share, %"}},
            "style": {"ratio": "16:9"}
        }))
        .unwrap();
        let rows: Vec<_> = [
            ("Ratings", 2.4),
            ("Search", 8.7),
            ("Weekly", 17.3),
            ("PyPI", 33.7),
            ("npm", 49.8),
            ("Firms", 52.4),
            ("HN", 64.9),
            ("Agents", 70.9),
        ]
        .iter()
        .map(|(measure, share)| json!({"measure": measure, "share": share}))
        .collect();
        let mut inside = 0;
        for width in PHONE {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            let panel = &g.panels[0];
            let (bars, values) = bars_and_values(panel);
            assert_eq!(values.len(), 8, "{width}: {:?}", g.dropped);
            let (o, t) = values.iter().find(|(_, t)| t.text == "70.9").unwrap();
            let bar = bars.iter().find(|(b, _)| b == o).unwrap().1;
            // Where the bar ends a label's width from the plot's edge, the value goes inside.
            if t.bounds.x < bar.right() {
                inside += 1;
                assert!(
                    (bar.right() - 4.0 - t.bounds.right()).abs() < 0.01,
                    "{width}: inside, 4 px in from the value end"
                );
                assert!((t.bounds.center()[1] - bar.center()[1]).abs() < 0.01);
            } else {
                assert!(t.bounds.x >= bar.right() + 3.0 - 0.01, "{width}: past the value end");
            }
        }
        assert!(inside > 0, "a narrow width leaves no room past the end");
    }

    #[test]
    fn the_blocked_value_of_a_close_pair_sits_inside_its_own_bar_end() {
        // "Safety": 20.3 and 21.4 end a label's width apart, so both cannot stand past their ends.
        let (block, rows) = grouped_bars();
        for width in [632.0, 656.0] {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            let panel = &g.panels[0];
            let (bars, values) = bars_and_values(panel);
            assert_eq!(values.len(), 16, "{width}: every value shows: {:?}", g.dropped);
            let place = |text: &str| {
                let (o, t) = values.iter().find(|(_, t)| t.text == text).unwrap();
                (bars.iter().find(|(b, _)| b == o).unwrap().1, t.bounds)
            };
            let (first, beside) = place("20.3");
            assert!(
                beside.x >= first.right() + 3.0 - 0.01,
                "{width}: '20.3' stands past its end"
            );
            let (second, within) = place("21.4");
            assert!(
                (second.right() - 4.0 - within.right()).abs() < 0.01,
                "{width}: '21.4' sits inside its bar, 4 px in from the value end"
            );
            assert!((within.center()[1] - second.center()[1]).abs() < 0.01);
        }
    }

    /// Two regions' changes per category, with a unit: signed grouped bars with long values.
    fn signed_grouped_bars() -> (ChartBlock, Vec<serde_json::Value>) {
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "T", "encoding": {"x": "x", "y": "y", "color": "series"},
            "axes": {"x": {"label": "Observation"}, "y": {"label": "Measured value", "unit": "MWh"}},
            "style": {"ratio": "16:9"}
        }))
        .unwrap();
        let rows = [
            ("Enterprise platform subscriptions", -46, -39),
            ("Local commercial partnerships", -23, -16),
            ("International audience expansion", 0, 7),
            ("Research and development", 23, 30),
        ]
        .iter()
        .flat_map(|(x, north, south)| {
            [
                json!({"x": x, "series": "North", "y": north}),
                json!({"x": x, "series": "South", "y": south}),
            ]
        })
        .collect();
        (block, rows)
    }

    #[test]
    fn a_value_inside_a_thin_bar_never_meets_another_value() {
        // Space Mono's and Patrick Hand's line boxes are taller than a thin grouped bar, so a
        // value inside its bar reaches past the bar toward the sibling's value. The label check
        // allows two line boxes 1 px of each other.
        let hatched = ChartTheme {
            data_font: "Patrick Hand, cursive".into(),
            decals: true,
            sign_hatch: true,
            ..ChartTheme::default()
        };
        let mono = |font: &str| ChartTheme {
            data_font: font.into(),
            ..ChartTheme::default()
        };
        let themes = [
            mono("Space Mono, ui-monospace, monospace"),
            hatched,
            mono("Geist Mono, ui-monospace, monospace"),
        ];
        for (block, rows) in [grouped_bars(), signed_grouped_bars()] {
            for theme in &themes {
                let font = &theme.data_font;
                let mut inside = 0;
                for width in (240..=900).step_by(2).map(f64::from) {
                    let g = layout(&block, &rows, theme, &opts(width));
                    let (bars, values) = bars_and_values(&g.panels[0]);
                    for (i, (o, t)) in values.iter().enumerate() {
                        // A zero has no bar.
                        inside += usize::from(
                            bars.iter()
                                .any(|(b, bar)| b == o && t.bounds.x >= bar.x && t.bounds.right() <= bar.right()),
                        );
                        for (_, other) in &values[i + 1..] {
                            assert!(
                                !t.bounds.overlaps(&other.bounds, 1.0),
                                "{font} at {width}: '{}' meets '{}'",
                                t.text,
                                other.text
                            );
                        }
                    }
                }
                assert!(inside > 0, "{font}: some value still goes inside its bar");
            }
        }
    }

    #[test]
    fn a_slid_value_leaves_its_siblings_inside_value_room() {
        // "Safety" in Space Mono: '20.3' slides up off the longer bar below it and '21.4' goes
        // inside that bar. Flush with its bar, '20.3' met '21.4' by more than the check allows.
        let (block, rows) = grouped_bars();
        let theme = ChartTheme {
            data_font: "Space Mono, ui-monospace, monospace".into(),
            ..ChartTheme::default()
        };
        for width in [300.0, 390.0, 656.0] {
            let g = layout(&block, &rows, &theme, &opts(width));
            let (bars, values) = bars_and_values(&g.panels[0]);
            let place = |text: &str| {
                let (o, t) = values
                    .iter()
                    .find(|(_, t)| t.text == text)
                    .unwrap_or_else(|| panic!("{width}: '{text}' prints: {:?}", g.dropped));
                (bars.iter().find(|(b, _)| b == o).unwrap().1, t.bounds)
            };
            let (first, beside) = place("20.3");
            let (second, within) = place("21.4");
            assert!(
                beside.x >= first.right() + 3.0 - 0.01,
                "{width}: '20.3' stands past its end"
            );
            assert!(within.right() <= second.right(), "{width}: '21.4' sits inside its bar");
            assert!(
                beside.bottom() <= within.y + 1.0,
                "{width}: the line boxes share at most 1 px"
            );
            let middle = beside.center()[1];
            assert!(
                first.y <= middle && middle <= first.bottom(),
                "{width}: '20.3' stays level with its bar"
            );
        }
    }

    /// The Titanic report's survival shares: two columns per class on an axis the author fixed
    /// from 0 to 100%.
    fn titanic_shares() -> (ChartBlock, Vec<serde_json::Value>) {
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "Women survived more often than men in every class",
            "encoding": {"x": "class", "y": "share", "color": "sex"},
            "axes": {"y": {"label": "Survived", "format": "percent", "min": 0, "max": 1}},
            "style": {"ratio": "2:1"}
        }))
        .unwrap();
        let shares = [
            ("1st", 0.97, 0.34),
            ("2nd", 0.88, 0.14),
            ("3rd", 0.46, 0.17),
            ("Crew", 0.87, 0.22),
        ];
        let rows = shares
            .iter()
            .flat_map(|(class, women, men)| {
                [
                    json!({"class": class, "sex": "Women", "share": women}),
                    json!({"class": class, "sex": "Men", "share": men}),
                ]
            })
            .collect();
        (block, rows)
    }

    #[test]
    fn a_fixed_maximum_keeps_the_axis_and_lowers_the_plot_for_the_tallest_value() {
        // '97%' on an axis the author ended at 100%: the axis cannot end a step higher, so on a
        // phone the value had no place under the axis title and was dropped.
        let (block, rows) = titanic_shares();
        for width in PHONE {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            let panel = &g.panels[0];
            let (bars, values) = bars_and_values(panel);
            let (o, t) = values
                .iter()
                .find(|(_, t)| t.text == "97%")
                .unwrap_or_else(|| panic!("{width}: '97%' prints: {:?}", g.dropped));
            let bar = bars.iter().find(|(b, _)| b == o).unwrap().1;
            assert!(
                t.bounds.inside(&bar, 0.0) || t.bounds.bottom() <= bar.y,
                "{width}: '97%' stands above its bar or inside it"
            );
            let texts: Vec<&str> = panel
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Text(t) => Some(t.text.as_str()),
                    _ => None,
                })
                .collect();
            assert!(
                texts.contains(&"100%"),
                "{width}: the axis still ends at the author's 100%"
            );
            assert!(
                !texts.iter().any(|t| ["110%", "120%", "125%", "150%"].contains(t)),
                "{width}: no tick past the author's maximum: {texts:?}"
            );
        }
    }

    #[test]
    fn a_wide_faced_value_slides_off_the_taller_column_beside_it() {
        // In Space Mono '34%' centred over the shorter column of a pair met the taller one's
        // clearance by a fraction of a pixel, and every men's share was dropped at 390.
        let (block, rows) = titanic_shares();
        let theme = ChartTheme {
            data_font: "Space Mono, ui-monospace, monospace".into(),
            ..ChartTheme::default()
        };
        for width in PHONE {
            let g = layout(&block, &rows, &theme, &opts(width));
            let (bars, values) = bars_and_values(&g.panels[0]);
            for text in ["34%", "14%", "17%"] {
                let (o, t) = values
                    .iter()
                    .find(|(_, t)| t.text == text)
                    .unwrap_or_else(|| panic!("{width}: '{text}' prints: {:?}", g.dropped));
                let bar = bars.iter().find(|(b, _)| b == o).unwrap().1;
                let centre = t.bounds.center()[0];
                assert!(
                    bar.x <= centre && centre <= bar.right(),
                    "{width}: '{text}' stays over its column"
                );
                // A column wide enough holds its value inside; only a value above it slides.
                if t.bounds.inside(&bar, 0.0) {
                    continue;
                }
                assert!(t.bounds.bottom() <= bar.y, "{width}: '{text}' stands above its column");
                let sibling = bars
                    .iter()
                    .map(|(_, r)| *r)
                    .find(|r| (bar.x - r.right()).abs() <= 1.5)
                    .expect("the taller column beside it");
                assert!(
                    t.bounds.x >= sibling.right() + 2.0,
                    "{width}: '{text}' keeps its clearance from the column beside it"
                );
            }
        }
    }

    #[test]
    fn a_value_wider_than_an_edge_column_slides_back_into_the_plot_over_its_bar() {
        // Eleven quarters of two bars each: on a phone a column is narrower than three digits.
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "T", "encoding": {"x": "quarter", "y": "statements", "color": "direction"},
            "axes": {"x": {"label": "Quarter"}, "y": {"label": "Statements"}},
            "style": {"ratio": "16:9"}
        }))
        .unwrap();
        let counts = [
            (3, 4),
            (14, 6),
            (18, 1),
            (11, 4),
            (10, 7),
            (16, 11),
            (38, 22),
            (32, 24),
            (92, 43),
            (65, 108),
            (37, 159),
        ];
        let rows: Vec<_> = counts
            .iter()
            .enumerate()
            .flat_map(|(q, (anthropic, openai))| {
                let quarter = format!("{} Q{}", 2024 + q / 4, q % 4 + 1);
                [
                    json!({"quarter": quarter, "direction": "To Anthropic", "statements": anthropic}),
                    json!({"quarter": quarter, "direction": "To OpenAI", "statements": openai}),
                ]
            })
            .collect();
        let mut slid = 0;
        for width in PHONE {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            let panel = &g.panels[0];
            let (bars, values) = bars_and_values(panel);
            let (o, t) = values
                .iter()
                .find(|(_, t)| t.text == "159")
                .unwrap_or_else(|| panic!("{width}: the tallest bar keeps its value: {:?}", g.dropped));
            let bar = bars.iter().find(|(b, _)| b == o).unwrap().1;
            let centre = t.bounds.center()[0];
            assert!(
                t.bounds.x >= panel.plot.x - 0.01 && t.bounds.right() <= panel.plot.right() + 0.01,
                "{width}: '159' stays in the plot"
            );
            assert!(
                bar.x <= centre && centre <= bar.right(),
                "{width}: '159' stands over its own bar"
            );
            assert!(
                (bar.y - t.bounds.bottom() - 3.0).abs() < 0.01,
                "{width}: 3 px above its bar"
            );
            slid += usize::from((centre - bar.center()[0]).abs() > 0.01);
        }
        assert!(slid > 0, "at some phone width the last column's value has to slide");
    }

    #[test]
    fn a_bar_value_stays_at_its_bar_when_a_shorter_shown_number_replaces_it() {
        // The runtime lays bars out over grid values ("2.41") and swaps in the numbers of a
        // second layout over the shown values ("2.4"). The shorter text stays attached the way
        // the longer one was placed: centred on a column, or 3 px from a bar's end.
        let lay = |orientation: &str, values: &[f64]| {
            let block = ChartBlock::from_value(&json!({
                "type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"},
                "style": {"orientation": orientation}
            }))
            .unwrap();
            let rows: Vec<_> = values
                .iter()
                .enumerate()
                .map(|(i, v)| json!({"c": format!("c{i}"), "v": v}))
                .collect();
            layout(&block, &rows, &ChartTheme::default(), &opts(632.0))
        };
        let (grid, shown) = (
            [2.41, 8.69, -17.31, 33.7, 49.8, 52.4],
            [2.4, 8.7, -17.3, 33.7, 49.8, 52.4],
        );
        for orientation in ["vertical", "horizontal"] {
            let mut g = lay(orientation, &grid);
            g.set_mark_texts(&lay(orientation, &shown).mark_texts());
            let (bars, values) = bars_and_values(&g.panels[0]);
            assert_eq!(values.len(), 6, "{orientation}: {:?}", g.dropped);
            assert!(
                values.iter().any(|(_, t)| t.text == "2.4"),
                "{orientation}: the shown numbers are swapped in"
            );
            for (o, t) in &values {
                let bar = bars.iter().find(|(b, _)| b == o).unwrap().1;
                let negative = t.text.starts_with('−');
                if orientation == "vertical" || t.bounds.inside(&bar, 0.0) {
                    assert!(
                        (t.bounds.center()[0] - bar.center()[0]).abs() < 0.01,
                        "{orientation}: '{}' is centred on its bar",
                        t.text
                    );
                } else if negative {
                    assert!(
                        (bar.x - 3.0 - t.bounds.right()).abs() < 0.01,
                        "{orientation}: '{}' ends 3 px left of its bar",
                        t.text
                    );
                } else {
                    assert!(
                        (t.bounds.x - bar.right() - 3.0).abs() < 0.01,
                        "{orientation}: '{}' starts 3 px right of its bar",
                        t.text
                    );
                }
            }
        }
    }

    #[test]
    fn a_line_value_stays_beside_its_point_when_a_shorter_shown_number_replaces_it() {
        // As for bars: a value placed over grid values ("8.18") gets the shown number ("8.2")
        // and keeps the side its place gave it: the middle above or below its point, the near
        // edge beside it.
        let block =
            ChartBlock::from_value(&json!({"type": "line", "title": "T", "encoding": {"x": "x", "y": "y"}})).unwrap();
        let lay = |values: &[f64]| {
            let rows: Vec<_> = values
                .iter()
                .enumerate()
                .map(|(i, v)| json!({"x": i, "y": v}))
                .collect();
            layout(&block, &rows, &ChartTheme::default(), &opts(632.0))
        };
        let values = |g: &crate::geometry::Geometry| -> Vec<(String, Rect, [f64; 2])> {
            let (panel, ids) = (&g.panels[0], g.mark_ids());
            panel
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Text(t) if t.role == Role::Label => {
                        let o = &panel.observations[ids.iter().position(|id| Some(id) == t.mark.as_ref())?];
                        Some((t.text.clone(), t.bounds, o.dots.first().map_or(o.at, |d| d.at)))
                    }
                    _ => None,
                })
                .collect()
        };
        let mut g = lay(&[2.41, 5.2, 1.69, 8.18, 4.3, 7.87]);
        let placed = values(&g);
        g.set_mark_texts(&lay(&[2.4, 5.2, 1.7, 8.2, 4.3, 7.9]).mark_texts());
        let shown = values(&g);
        assert!(
            placed.len() >= 3,
            "the last, lowest and highest value show: {:?}",
            g.dropped
        );
        assert_eq!(shown.len(), placed.len());
        let (mut centred, mut beside) = (0, 0);
        for ((before, was, at), (text, now, _)) in placed.iter().zip(&shown) {
            assert!(text.len() < before.len(), "'{before}' became the shorter '{text}'");
            if (was.center()[0] - at[0]).abs() < 0.5 {
                centred += 1;
                assert!(
                    (now.center()[0] - at[0]).abs() < 0.01,
                    "'{text}' stays centred on its point"
                );
            } else if was.x > at[0] {
                beside += 1;
                assert!((now.x - was.x).abs() < 0.01, "'{text}' starts where '{before}' did");
            } else {
                beside += 1;
                assert!(
                    (now.right() - was.right()).abs() < 0.01,
                    "'{text}' ends where '{before}' did"
                );
            }
        }
        assert!(
            centred > 0 && beside > 0,
            "{centred} centred, {beside} beside: {placed:?}"
        );
    }

    /// The y axis' tick labels, bottom to top.
    fn value_ticks(panel: &crate::geometry::Panel) -> Vec<String> {
        let mut ticks: Vec<&Text> = panel
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text(t) if t.role == Role::AxisLabel && t.bounds.right() <= panel.plot.x => Some(t),
                _ => None,
            })
            .collect();
        ticks.sort_by(|a, b| b.bounds.y.total_cmp(&a.bounds.y));
        ticks.into_iter().map(|t| t.text.clone()).collect()
    }

    #[test]
    fn the_tallest_bar_keeps_room_for_its_value_under_the_axis_title() {
        // Three products over four months on a phone: the bars are too thin to hold their
        // values, and "73.3" on an axis that ends at 75 has only the axis title above it. The
        // axis ends a step higher, so the tallest bar's value stands above its bar.
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "T", "encoding": {"x": "as_of", "y": "share", "color": "product"},
            "axes": {"x": {"label": "Month"}, "y": {"label": "Share of gen-AI web visits, %"}},
            "style": {"ratio": "16:9"}
        }))
        .unwrap();
        let rows: Vec<_> = [
            ("2025-08", [1.9, 73.3, 12.9]),
            ("2026-03", [6.0, 56.7, 25.4]),
            ("2026-06", [9.2, 52.7, 27.8]),
            ("2026-08", [9.3, 55.5, 25.6]),
        ]
        .iter()
        .flat_map(|(month, shares)| {
            ["Claude", "ChatGPT", "Gemini"]
                .iter()
                .zip(shares)
                .map(move |(product, share)| json!({"as_of": month, "product": product, "share": share}))
        })
        .collect();
        let mut stepped = 0;
        for width in PHONE {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            let panel = &g.panels[0];
            let (bars, values) = bars_and_values(panel);
            let (o, t) = values
                .iter()
                .find(|(_, t)| t.text == "73.3")
                .unwrap_or_else(|| panic!("{width}: the tallest bar's value shows: {:?}", g.dropped));
            let bar = bars.iter().find(|(b, _)| b == o).unwrap().1;
            assert!(
                (bar.y - t.bounds.bottom() - 3.0).abs() < 0.01 && (t.bounds.center()[0] - bar.center()[0]).abs() < 0.01,
                "{width}: 3 px above its own bar"
            );
            let title = panel
                .items
                .iter()
                .find_map(|i| match i {
                    Item::Text(t) if t.role == Role::AxisTitle && t.bounds.y < panel.plot.y => Some(t.bounds),
                    _ => None,
                })
                .unwrap();
            assert!(t.bounds.y >= title.bottom() + 2.0, "{width}: clear of the axis title");
            stepped += usize::from(value_ticks(panel).last().is_some_and(|top| top == "100"));
        }
        assert!(stepped > 0, "at some phone width the axis ends a step higher for it");
    }

    #[test]
    fn a_tallest_bar_beside_the_axis_title_keeps_its_axis() {
        // The same thin bars with the tallest in the last month, right of a short axis title:
        // its value has a clear place above the plot, so the axis keeps its last tick.
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "T", "encoding": {"x": "as_of", "y": "share", "color": "product"},
            "axes": {"x": {"label": "Month"}, "y": {"label": "Share, %"}},
            "style": {"ratio": "16:9"}
        }))
        .unwrap();
        let rows: Vec<_> = [
            ("2025-08", [9.3, 55.5, 25.6]),
            ("2026-03", [6.0, 56.7, 25.4]),
            ("2026-06", [9.2, 52.7, 27.8]),
            ("2026-08", [1.9, 73.3, 12.9]),
        ]
        .iter()
        .flat_map(|(month, shares)| {
            ["Claude", "ChatGPT", "Gemini"]
                .iter()
                .zip(shares)
                .map(move |(product, share)| json!({"as_of": month, "product": product, "share": share}))
        })
        .collect();
        let g = layout(&block, &rows, &ChartTheme::default(), &opts(318.0));
        let panel = &g.panels[0];
        let (bars, values) = bars_and_values(panel);
        let (o, t) = values.iter().find(|(_, t)| t.text == "73.3").expect("the value shows");
        let bar = bars.iter().find(|(b, _)| b == o).unwrap().1;
        assert!((bar.y - t.bounds.bottom() - 3.0).abs() < 0.01, "3 px above its bar");
        assert_eq!(value_ticks(panel).last().map(String::as_str), Some("75"));
    }

    #[test]
    fn bars_that_hold_their_values_keep_their_axis() {
        // Four wide bars on a phone: "78.5" sits inside its bar, so the axis still ends at 80,
        // 3 px above the tallest bar.
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"},
            "axes": {"y": {"label": "Share, %"}},
            "style": {"orientation": "vertical", "ratio": "16:9"}
        }))
        .unwrap();
        let rows: Vec<_> = [("a", 31.2), ("b", 78.5), ("c", 44.0), ("d", 52.6)]
            .iter()
            .map(|(c, v)| json!({"c": c, "v": v}))
            .collect();
        for width in PHONE {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            let panel = &g.panels[0];
            let (bars, values) = bars_and_values(panel);
            let (o, t) = values.iter().find(|(_, t)| t.text == "78.5").unwrap();
            let bar = bars.iter().find(|(b, _)| b == o).unwrap().1;
            assert!(t.bounds.inside(&bar, 0.0), "{width}: the value sits inside its bar");
            assert_eq!(value_ticks(panel).last().map(String::as_str), Some("80"), "{width}");
        }
    }

    #[test]
    fn a_vertical_bar_value_keeps_to_its_column_across_a_gridline() {
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"},
            "style": {"orientation": "vertical"}
        }))
        .unwrap();
        let rows: Vec<_> = [
            ("A", 95),
            ("B", 145),
            ("C", 195),
            ("D", -45),
            ("E", 245),
            ("F", 295),
            ("G", 345),
            ("H", 395),
        ]
        .iter()
        .map(|(c, v)| json!({"c": c, "v": v}))
        .collect();
        let mut crossed = 0;
        for width in PHONE.into_iter().chain([560.0]) {
            let g = layout(&block, &rows, &ChartTheme::default(), &opts(width));
            let panel = &g.panels[0];
            let (bars, values) = bars_and_values(panel);
            let lines: Vec<(f64, Rings)> = panel
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Line {
                        path,
                        role: Role::Grid | Role::Baseline,
                        stroke,
                        ..
                    } => Some((stroke.width / 2.0, path.flatten(0.25))),
                    _ => None,
                })
                .collect();
            for (o, t) in &values {
                let bar = bars.iter().find(|(b, _)| b == o).unwrap().1;
                if t.bounds.inside(&bar, 0.0) {
                    continue;
                }
                assert!(
                    (t.bounds.center()[0] - bar.center()[0]).abs() < 0.01,
                    "{width}: '{}' is centred on its column",
                    t.text
                );
                let gap = if t.text.starts_with('−') {
                    t.bounds.y - bar.bottom()
                } else {
                    bar.y - t.bounds.bottom()
                };
                assert!(
                    (gap - 3.0).abs() < 0.01,
                    "{width}: '{}' sits 3 px past its value end",
                    t.text
                );
                for (half, rings) in &lines {
                    let y = rings[0][0][1];
                    if t.bounds.y < y && y < t.bounds.bottom() && rings.iter().all(|r| r[0][1] == y) {
                        crossed += 1;
                    }
                    for ring in rings {
                        assert!(
                            !crate::layout::place::segment_touches_rect(ring[0], ring[1], &t.bounds.inflate(*half)),
                            "{width}: a line runs through '{}'",
                            t.text
                        );
                    }
                }
            }
        }
        assert!(crossed > 0, "some value stands across a gridline");
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

    #[test]
    fn decades_thin_by_the_names_printed_not_by_their_numbers() {
        // Fifteen decades step by ten, so "every 5" of their numbers is every one of them: all
        // fifteen were printed over each other while the layout gave each five bands of room.
        let block = ChartBlock::from_value(&json!({
            "type": "bar", "title": "Fifteen decades", "encoding": {"x": "decade", "y": "mean"},
            "axes": {"y": {"label": "Mean anomaly", "unit": "°C"}},
            "style": {"ratio": "2:1"}
        }))
        .unwrap();
        let means = [
            -0.21, -0.24, -0.31, -0.33, -0.24, -0.12, 0.04, -0.05, -0.03, 0.04, 0.25, 0.39, 0.59, 0.80, 1.06,
        ];
        let rows: Vec<_> = means
            .iter()
            .enumerate()
            .map(|(i, m)| json!({"decade": format!("{}s", 1880 + 10 * i), "mean": m}))
            .collect();
        let themes = [
            ChartTheme::default(),
            ChartTheme {
                data_font: "Patrick Hand, cursive".into(),
                ..ChartTheme::default()
            },
        ];
        for theme in &themes {
            for width in (240..=900).step_by(2).map(f64::from) {
                let g = layout(&block, &rows, theme, &opts(width));
                let mut names: Vec<&Text> = g.panels[0]
                    .items
                    .iter()
                    .filter_map(|i| match i {
                        Item::Text(t) if t.role == Role::AxisLabel && t.text.ends_with("0s") => Some(t),
                        _ => None,
                    })
                    .collect();
                names.sort_by(|a, b| a.bounds.x.total_cmp(&b.bounds.x));
                assert!(names.len() >= 2, "{width}: at least two decades are named");
                for pair in names.windows(2) {
                    assert!(
                        pair[1].bounds.x - pair[0].bounds.right() >= 4.0,
                        "{width}: '{}' and '{}' stand apart",
                        pair[0].text,
                        pair[1].text
                    );
                }
                assert!(
                    !g.checks.iter().any(|c| c.kind == "label-collision"),
                    "{width}: {:?}",
                    g.checks
                );
                let thinned = g.checks.iter().find(|c| c.kind == "label-thinned");
                assert_eq!(
                    thinned.is_some(),
                    names.len() < 15,
                    "{width}: thinning is said when it happens"
                );
                if let Some(check) = thinned {
                    let decade = |t: &Text| t.text[..4].parse::<usize>().unwrap() / 10;
                    let apart = names.windows(2).map(|w| decade(w[1]) - decade(w[0])).min().unwrap();
                    assert!(
                        check
                            .message
                            .starts_with(&format!("Category labels show every {apart}")),
                        "{width}: {} names, '{}'",
                        names.len(),
                        check.message
                    );
                }
            }
        }
    }
}
