//! The frame shared by charts that set categories against one value axis (waterfall, lollipop,
//! funnel, candlestick and the like): panel sizes, the value axis with its grid and baseline, and
//! category names that wrap and are never cut (FEA-CH-19, FEA-CH-24, FEA-CH-26). A painter plans
//! once with [`plan`], builds each panel's frame with [`frame`], draws its marks on that frame and
//! closes with [`finish`].

use super::{BASELINE_WIDTH, Ctx, FACET_GAP_Y, GRID_WIDTH, LABEL_GAP, MIN_PLOT, Painted, Style};
use crate::axis::{self, CategoryLayout, ValueAxis, ValueSpec, Zero};
use crate::block::Orientation;
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Item, Legend, Observation, Panel, Path, Rect, Role, Stroke, Swatch, Targeting,
};
use crate::numbers::Format;
use serde_json::json;

/// Most lines a horizontal category name wraps to (FEA-CH-26).
const NAME_LINES: usize = 4;

/// What the chart as a whole decided before any panel is drawn.
pub(super) struct Plan {
    pub horizontal: bool,
    pub columns: usize,
    /// Width of one panel.
    pub panel_h: f64,
    /// Room above a panel's plot for its facet title.
    pub title_room: f64,
    /// Reader forms of the category names, in axis order.
    pub names: Vec<String>,
    thin_unfit: bool,
    ordered: Option<axis::Ordered>,
    gutter_share: f64,
    widest: f64,
    /// Colours of the value axis titles (first, second) when a title names one series, so the
    /// axis pairs with its marks by colour; `None` keeps the ink.
    pub title_colors: [Option<crate::color::Oklch>; 2],
    /// Pixels the value axis keeps above the largest value, for labels set beyond the marks.
    pub headroom: f64,
}

/// Choose orientation and panel size. `per_name` is how many name rows each category needs
/// when horizontal (grouped marks need more than one).
pub(super) fn plan(ctx: &Ctx, panels: usize, names: &[String], per_name: f64, default_ratio: f64) -> Plan {
    plan_inner(ctx, panels, names, per_name, default_ratio, false, false, false)
}

/// [`plan`] for a type whose automatic orientation is horizontal (`horizontal_auto`), as a
/// bullet chart's is; an author's `style.orientation` still wins.
pub(super) fn plan_oriented(
    ctx: &Ctx,
    panels: usize,
    names: &[String],
    per_name: f64,
    default_ratio: f64,
    horizontal_auto: bool,
) -> Plan {
    plan_inner(
        ctx,
        panels,
        names,
        per_name,
        default_ratio,
        horizontal_auto,
        false,
        false,
    )
}

/// [`plan`] for charts whose categories run along time (`along_time`): names that do not fit never
/// turn the chart horizontal on their own, only `style.orientation` does, and names that fit
/// nowhere, even thinned by period, are thinned evenly instead of overlapping.
pub(super) fn plan_with(
    ctx: &Ctx,
    panels: usize,
    names: &[String],
    per_name: f64,
    default_ratio: f64,
    along_time: bool,
) -> Plan {
    plan_inner(ctx, panels, names, per_name, default_ratio, false, along_time, false)
}

/// [`plan`] for charts that draw a second value axis (see [`frame_with`]): a horizontal plan
/// makes room for that axis above the plot.
pub(super) fn plan_secondary(ctx: &Ctx, panels: usize, names: &[String], per_name: f64, default_ratio: f64) -> Plan {
    plan_inner(ctx, panels, names, per_name, default_ratio, false, false, true)
}

#[allow(clippy::too_many_arguments)]
fn plan_inner(
    ctx: &Ctx,
    panels: usize,
    names: &[String],
    per_name: f64,
    default_ratio: f64,
    horizontal_auto: bool,
    along_time: bool,
    secondary: bool,
) -> Plan {
    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let size = ctx.size();
    let title_room = if ctx.block.encoding.facet.is_some() {
        lh + 6.0
    } else {
        0.0
    };
    let columns = super::facet_columns(panels, width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let ordered = axis::ordered(names, ctx.locale);
    let reader = ordered.as_ref().map_or_else(|| names.to_vec(), |o| o.text.clone());
    let horizontal = match ctx.block.style.orientation {
        Orientation::Horizontal => true,
        Orientation::Vertical => false,
        Orientation::Auto if horizontal_auto => true,
        Orientation::Auto => {
            !along_time
                && ordered.is_none()
                && matches!(
                    axis::category_layout(
                        &reader,
                        None,
                        // The room `frame_with` measures names against.
                        (pw - 80.0 - if secondary { 40.0 } else { 0.0 }).max(1.0) / names.len().max(1) as f64,
                        &ctx.style.font,
                        size
                    ),
                    CategoryLayout::Unfit { .. }
                )
        }
    };
    let widest = reader.iter().map(|n| ctx.style.font.width(n, size)).fold(0.0, f64::max);
    let mut plan = Plan {
        horizontal,
        columns,
        panel_h: 0.0,
        title_room,
        names: reader,
        thin_unfit: along_time,
        ordered,
        gutter_share: 0.4,
        widest,
        title_colors: [None; 2],
        headroom: 0.0,
    };
    let ratio = ctx.block.style.ratio.unwrap_or(default_ratio);
    let mut h = (pw / ratio + title_room).max(MIN_PLOT + 3.0 * lh + title_room);
    if horizontal {
        let room = (plan.gutter(pw) - LABEL_GAP).max(1.0);
        let lines = plan
            .names
            .iter()
            .map(|n| ctx.style.font.wrap(n, size, room, NAME_LINES).len())
            .max()
            .unwrap_or(1)
            .clamp(1, NAME_LINES) as f64;
        let each = (lh * 1.6 * per_name.max(1.0)).max(lines * lh + lh * 0.6);
        h = h.max(plan.names.len() as f64 * each + 3.0 * lh + title_room + if secondary { 2.0 * lh } else { 0.0 });
    }
    plan.panel_h = h;
    plan
}

impl Plan {
    /// Width of the name gutter of a horizontal chart.
    fn gutter(&self, panel_width: f64) -> f64 {
        (self.widest + LABEL_GAP).min(panel_width * self.gutter_share)
    }
    /// Frames of the facet grid.
    pub fn frames(&self, count: usize, width: f64) -> Vec<Rect> {
        super::grid_frames(count, width, 0.0, self.panel_h)
    }
}

/// What the value axis shows.
pub(super) struct ValueSide<'a> {
    pub data: [f64; 2],
    pub zero: Zero,
    pub title: String,
    pub format: &'a Format,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// A second value axis: its own scale, tick labels and title, at the right of a vertical chart
/// and above the plot of a horizontal one.
/// Gridlines come from the primary axis only, so the two scales never draw competing grids.
pub(super) struct SecondarySide<'a> {
    pub data: [f64; 2],
    pub zero: Zero,
    pub title: String,
    pub format: &'a Format,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// One panel's drawn frame: axes, grid, names, and the maps marks are placed with.
pub(super) struct Frame {
    pub plot: Rect,
    pub band: f64,
    pub value: ValueAxis,
    /// The second value axis at the right, when the chart asked for one.
    pub secondary: Option<ValueAxis>,
    pub horizontal: bool,
    /// Grid, baseline, axis labels and title, in paint order; marks go after them.
    pub items: Vec<Item>,
}

impl Frame {
    /// Centre of category `c` along the category axis.
    pub fn center(&self, c: usize) -> f64 {
        let origin = if self.horizontal { self.plot.y } else { self.plot.x };
        origin + (c as f64 + 0.5) * self.band
    }
    /// Pixel position of value `v` along the value axis.
    pub fn at(&self, v: f64) -> f64 {
        if self.horizontal {
            self.value.map(v, self.plot.x, self.plot.right())
        } else {
            self.value.map(v, self.plot.bottom(), self.plot.y)
        }
    }
    /// Pixel position of `v` on the secondary axis; the primary position when there is none.
    pub fn at2(&self, v: f64) -> f64 {
        let Some(a) = &self.secondary else { return self.at(v) };
        if self.horizontal {
            a.map(v, self.plot.x, self.plot.right())
        } else {
            a.map(v, self.plot.bottom(), self.plot.y)
        }
    }
    /// Category position and a secondary value to a point.
    pub fn pt2(&self, c: f64, v: f64) -> [f64; 2] {
        if self.horizontal {
            [self.at2(v), c]
        } else {
            [c, self.at2(v)]
        }
    }
    /// Category position and value to a point.
    pub fn pt(&self, c: f64, v: f64) -> [f64; 2] {
        if self.horizontal {
            [self.at(v), c]
        } else {
            [c, self.at(v)]
        }
    }
    /// The box between categories positions `c0` and `c1` and values `v0` and `v1`.
    pub fn span(&self, c0: f64, c1: f64, v0: f64, v1: f64) -> Rect {
        let (a, b) = (self.at(v0), self.at(v1));
        if self.horizontal {
            Rect::new(a.min(b), c0, (a - b).abs(), c1 - c0)
        } else {
            Rect::new(c0, a.min(b), c1 - c0, (a - b).abs())
        }
    }
    /// Mark thickness for `groups` marks side by side in one band, at most `cap` pixels each.
    pub fn thickness(&self, groups: usize, cap: f64) -> f64 {
        (self.band * 0.7 / groups.max(1) as f64).min(cap)
    }
    /// The start of mark `slot` of `groups` along the category axis, for `thickness` wide marks.
    pub fn slot_start(&self, c: usize, slot: usize, groups: usize, thickness: f64) -> f64 {
        let inner = self.band * 0.7;
        let center = self.center(c);
        if groups <= 1 {
            center - thickness / 2.0
        } else {
            let each = inner / groups as f64;
            center - inner / 2.0 + slot as f64 * each + (each - thickness) / 2.0
        }
    }
}

/// Draw the frame of the panel in `frame`: the value axis with grid, baseline and title, and the
/// category names. `first` is the first panel, which reports name layout findings once.
pub(super) fn frame(ctx: &mut Ctx, plan: &Plan, outer: &Rect, side: &ValueSide, first: bool) -> Frame {
    frame_with(ctx, plan, outer, side, None, first)
}

/// [`frame`] with an optional second value axis: at the right of a vertical plan, above the plot
/// of a horizontal one (which wants [`plan_secondary`] for the room). Its labels and title are
/// drawn here; gridlines come from the primary axis only.
pub(super) fn frame_with(
    ctx: &mut Ctx,
    plan: &Plan,
    outer: &Rect,
    side: &ValueSide,
    secondary: Option<&SecondarySide>,
    first: bool,
) -> Frame {
    let lh = ctx.line_height();
    let size = ctx.size();
    let font = ctx.style.font.clone();
    let (ascent, _) = font.vertical();
    let horizontal = plan.horizontal;
    let n = plan.names.len().max(1);
    let gutter = if horizontal { plan.gutter(outer.w) } else { 0.0 };
    let top =
        outer.y + plan.title_room + lh + 4.0 + lh / 2.0 + if secondary.is_some() && horizontal { lh } else { 0.0 };
    let right_extra = if secondary.is_some() && !horizontal { 40.0 } else { 0.0 };
    let estimate = (outer.w - 80.0 - right_extra).max(1.0) / n as f64;
    let layout = if horizontal {
        CategoryLayout::OneLine
    } else {
        axis::category_layout_with(&plan.names, plan.ordered.as_ref(), estimate, &font, size, true)
    };
    let layout = if plan.thin_unfit && matches!(layout, CategoryLayout::Unfit { .. }) {
        // Names fit nowhere: show every k-th, judged against the plot the value axis leaves.
        let probe = axis::value_axis(&ValueSpec {
            data: side.data,
            zero: side.zero,
            min: side.min,
            max: side.max,
            length: (outer.bottom() - top - lh - LABEL_GAP).max(MIN_PLOT * 0.5),
            horizontal: false,
            format: side.format,
            font: &ctx.style.figures,
            size,
        });
        let lead = probe.widest(&ctx.style.figures, size) + LABEL_GAP;
        match thin_evenly(&plan.names, &font, size, lead, outer.w - lead - 6.0) {
            Some((every, lines)) => CategoryLayout::Thinned { every, lines },
            None => layout,
        }
    } else {
        layout
    };
    let name_rows = match &layout {
        CategoryLayout::Wrapped(l)
        | CategoryLayout::Thinned { lines: l, .. }
        | CategoryLayout::Unfit { lines: l, .. } => l.iter().map(Vec::len).max().unwrap_or(1).max(1),
        CategoryLayout::OneLine => 1,
    } as f64;
    let bottom_room = name_rows * lh + LABEL_GAP + if horizontal { lh } else { 0.0 };
    let plot_h = (outer.bottom() - top - bottom_room).max(MIN_PLOT * 0.5);
    let value_len = if horizontal { outer.w - gutter - 16.0 } else { plot_h };
    let value = axis::value_axis_room(
        &ValueSpec {
            data: side.data,
            zero: side.zero,
            min: side.min,
            max: side.max,
            length: value_len,
            horizontal,
            format: side.format,
            font: &ctx.style.figures,
            size,
        },
        plan.headroom,
    );
    let value2 = secondary.map(|s2| {
        axis::value_axis(&ValueSpec {
            data: s2.data,
            zero: s2.zero,
            min: s2.min,
            max: s2.max,
            length: value_len,
            horizontal,
            format: s2.format,
            font: &ctx.style.figures,
            size,
        })
    });
    let right_room = value2
        .as_ref()
        .filter(|_| !horizontal)
        .map_or(6.0, |a| a.widest(&ctx.style.figures, size) + LABEL_GAP + 2.0);
    let plot = if horizontal {
        let first_w = value
            .labels
            .first()
            .map_or(0.0, |l| ctx.style.figures.width(l, size) / 2.0);
        let last_w = value
            .labels
            .last()
            .into_iter()
            .chain(value2.iter().flat_map(|a| a.labels.last()))
            .map(|l| ctx.style.figures.width(l, size) / 2.0)
            .fold(0.0, f64::max);
        let left = outer.x + gutter.max(first_w);
        Rect::new(left, top, outer.right() - left - last_w.max(4.0), plot_h)
    } else {
        let left = outer.x + value.widest(&ctx.style.figures, size) + LABEL_GAP;
        Rect::new(left, top, outer.right() - left - right_room, plot_h)
    };
    let mut items = Vec::new();
    grid(ctx, plot, &value, horizontal, &mut items);
    let ink = ctx.style.ink;
    // Axis titles take the soft ink; one that names a single series takes that series' colour.
    let tint = |mut t: crate::geometry::Text, c: Option<crate::color::Oklch>| {
        if let Some(c) = c {
            t.color = Style::rgba(c);
        }
        t
    };
    if horizontal {
        items.push(Item::Text(tint(
            ctx.text(
                &side.title,
                plot.right(),
                plot.bottom() + LABEL_GAP + lh + ascent * size,
                Anchor::End,
                Face::Data,
                Role::AxisTitle,
                ink,
            ),
            plan.title_colors[0],
        )));
    } else {
        items.push(Item::Text(tint(
            ctx.text(
                &side.title,
                outer.x,
                top - lh / 2.0 - 6.0,
                Anchor::Start,
                Face::Data,
                Role::AxisTitle,
                ink,
            ),
            plan.title_colors[0],
        )));
    }
    if let (Some(s2), Some(axis2)) = (secondary, &value2) {
        for (t, label) in axis2.ticks.iter().zip(&axis2.labels) {
            items.push(Item::Text(if horizontal {
                ctx.text_mid(
                    label,
                    axis2.map(*t, plot.x, plot.right()),
                    plot.y - LABEL_GAP - lh / 2.0,
                    Anchor::Middle,
                    Face::Figures,
                    Role::AxisLabel,
                    ink,
                )
            } else {
                ctx.text_mid(
                    label,
                    plot.right() + LABEL_GAP,
                    axis2.map(*t, plot.bottom(), plot.y),
                    Anchor::Start,
                    Face::Figures,
                    Role::AxisLabel,
                    ink,
                )
            }));
        }
        let title = if horizontal {
            ctx.text_mid(
                &s2.title,
                plot.right(),
                plot.y - LABEL_GAP - lh * 1.5,
                Anchor::End,
                Face::Data,
                Role::AxisTitle,
                ink,
            )
        } else {
            ctx.text(
                &s2.title,
                outer.right(),
                top - lh / 2.0 - 6.0,
                Anchor::End,
                Face::Data,
                Role::AxisTitle,
                ink,
            )
        };
        let title = tint(title, plan.title_colors[1]);
        // A title that would run into the primary one is dropped and reported, never overlapped.
        let clear = items
            .iter()
            .filter_map(|i| match i {
                Item::Text(t) if t.role == Role::AxisTitle => Some(t.bounds),
                _ => None,
            })
            .all(|b| !overlaps(&b.inflate(4.0), &title.bounds));
        if clear {
            items.push(Item::Text(title));
        } else {
            ctx.drop_label(&s2.title, DropReason::Density, "axis-title", Some(title.bounds));
        }
    }
    let band = if horizontal { plot.h } else { plot.w } / n as f64;
    let room = (gutter - LABEL_GAP).max(1.0);
    let lines: Vec<Vec<String>> = match layout {
        CategoryLayout::OneLine if horizontal => {
            let wrapped: Vec<Vec<String>> = plan
                .names
                .iter()
                .map(|r| font.wrap(r, size, room, NAME_LINES))
                .collect();
            let unfit: Vec<&String> = plan
                .names
                .iter()
                .zip(&wrapped)
                .filter(|(_, l)| l.len() > NAME_LINES || l.iter().any(|line| font.width(line, size) > room + 0.5))
                .map(|(r, _)| r)
                .collect();
            if first {
                unfit_check(ctx, &unfit, "Shorten the names.");
            }
            wrapped
        }
        CategoryLayout::OneLine => plan.names.iter().map(|r| vec![r.clone()]).collect(),
        CategoryLayout::Wrapped(l) => l,
        CategoryLayout::Thinned { every, lines } => {
            if first {
                ctx.check(
                    "label-thinned",
                    Level::Info,
                    format!("Category names show every {}", axis::nth_printed(&lines)),
                    None,
                    Some("chart.encoding.x".into()),
                    None,
                    json!({"every": every}),
                );
            }
            lines
        }
        CategoryLayout::Unfit { lines, unfit } => {
            if first {
                let names: Vec<&String> = unfit.iter().map(|&i| &plan.names[i]).collect();
                unfit_check(ctx, &names, "Shorten the names, or set style.orientation: horizontal.");
            }
            lines
        }
    };
    for (c, l) in lines.iter().enumerate() {
        let center = if horizontal { plot.y } else { plot.x } + (c as f64 + 0.5) * band;
        for (k, line) in l.iter().enumerate().filter(|(_, line)| !line.is_empty()) {
            let t = if horizontal {
                ctx.text_mid(
                    line,
                    plot.x - LABEL_GAP,
                    center + (k as f64 - (l.len() as f64 - 1.0) / 2.0) * lh,
                    Anchor::End,
                    Face::Data,
                    Role::AxisLabel,
                    ink,
                )
            } else {
                ctx.text(
                    line,
                    center,
                    plot.bottom() + LABEL_GAP + ascent * size + k as f64 * lh,
                    Anchor::Middle,
                    Face::Data,
                    Role::AxisLabel,
                    ink,
                )
            };
            items.push(Item::Text(t));
        }
    }
    Frame {
        plot,
        band,
        value,
        secondary: value2,
        horizontal,
        items,
    }
}

/// The smallest step at which every `every`-th name fits the room of `every` bands and stays
/// inside the figure, starting at the earliest name that lets it, with the names between them
/// left blank. `lead` is the space left of the
/// plot and `width` the plot's width.
fn thin_evenly(
    names: &[String],
    font: &crate::text::Font,
    size: f64,
    lead: f64,
    width: f64,
) -> Option<(usize, Vec<Vec<String>>)> {
    let gap = size * 0.5;
    let band = width / names.len().max(1) as f64;
    let n = names.len();
    (2..=n).find_map(|every| {
        (0..every).find_map(|offset| {
            let fits = (offset..n).step_by(every).all(|i| {
                let w = font.width(&names[i], size);
                let center = lead + (i as f64 + 0.5) * band;
                w + gap <= band * every as f64 && center - w / 2.0 >= 0.0 && center + w / 2.0 <= lead + width + 6.0
            });
            fits.then(|| {
                let lines = names
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        if i % every == offset {
                            vec![name.clone()]
                        } else {
                            vec![]
                        }
                    })
                    .collect();
                (every, lines)
            })
        })
    })
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

fn unfit_check(ctx: &mut Ctx, unfit: &[&String], fix: &str) {
    if unfit.is_empty() {
        return;
    }
    ctx.check(
        "label-unfit",
        Level::Warning,
        format!(
            "{} category names do not fit their space: {}",
            unfit.len(),
            unfit.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ),
        Some(fix),
        Some("chart.encoding.x".into()),
        None,
        json!({"names": unfit}),
    );
}

/// Value gridlines, tick labels and the zero baseline.
fn grid(ctx: &Ctx, plot: Rect, axis: &ValueAxis, horizontal: bool, items: &mut Vec<Item>) {
    let ink = ctx.style.ink;
    let (ascent, _) = ctx.style.font.vertical();
    let rule = Style::rgba(ctx.style.rule_soft);
    for (t, label) in axis.ticks.iter().zip(&axis.labels) {
        if horizontal {
            let x = axis.map(*t, plot.x, plot.right());
            if *t != 0.0 {
                items.push(Item::Line {
                    path: Path::polyline(&[[x, plot.y], [x, plot.bottom()]]),
                    stroke: Stroke::solid(rule, GRID_WIDTH),
                    role: Role::Grid,
                    series: None,
                    obs: None,
                });
            }
            items.push(Item::Text(ctx.text(
                label,
                x,
                plot.bottom() + LABEL_GAP + ascent * ctx.size(),
                Anchor::Middle,
                Face::Figures,
                Role::AxisLabel,
                ink,
            )));
        } else {
            let y = axis.map(*t, plot.bottom(), plot.y);
            if *t != 0.0 {
                items.push(Item::Line {
                    path: Path::polyline(&[[plot.x, y], [plot.right(), y]]),
                    stroke: Stroke::solid(rule, GRID_WIDTH),
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
    }
    if axis.lo <= 0.0 && axis.hi >= 0.0 {
        let line = if horizontal {
            let x = axis.map(0.0, plot.x, plot.right());
            [[x, plot.y], [x, plot.bottom()]]
        } else {
            let y = axis.map(0.0, plot.bottom(), plot.y);
            [[plot.x, y], [plot.right(), y]]
        };
        items.push(Item::Line {
            path: Path::polyline(&line),
            stroke: Stroke::solid(Style::rgba(ink), BASELINE_WIDTH),
            role: Role::Baseline,
            series: None,
            obs: None,
        });
    }
}

/// A panel from a drawn frame, its marks and observations, with the facet title.
pub(super) fn panel(
    ctx: &Ctx,
    outer: &Rect,
    frame: Frame,
    marks: Vec<Item>,
    observations: Vec<Observation>,
    name: &Option<String>,
) -> Panel {
    let mut items = frame.items;
    items.extend(marks);
    let mut panel = Panel {
        facet: None,
        frame: *outer,
        plot: frame.plot,
        items,
        observations,
        targeting: Targeting::Contain,
        x: None,
        failure: None,
    };
    super::panel_title(ctx, &mut panel, name);
    panel
}

/// The legend of a chart whose colour channel names several series; a series the author's
/// highlight mutes has no entry (TYP-78).
pub(super) fn legend(ctx: &Ctx, swatch: Swatch) -> Option<Legend> {
    (ctx.series.len() > 1).then(|| Legend {
        swatch,
        entries: (0..ctx.series.len()).filter(|&s| !ctx.mutes_series(s)).collect(),
        bottom: ctx.style.legend_bottom,
    })
}

/// Close the chart: its height from the plan, the panels and the legend.
pub(super) fn finish(plan: &Plan, count: usize, panels: Vec<Panel>, legend: Option<Legend>) -> Painted {
    Painted {
        height: super::grid_height(count, plan.columns, plan.panel_h, FACET_GAP_Y),
        panels,
        figure_items: Vec::new(),
        legend,
        failure: None,
    }
}

/// A chart over one of its declared limits is not drawn (FEA-CH-75): the error names the limit and
/// the count, and the returned reason is what the panel shows instead of the chart. Nothing is
/// dropped, thinned or folded below the limit, so totals and shares always cover all the data.
/// `chart` and `what` name an entry of [`crate::limits::LIMITS`].
#[allow(dead_code)] // the painters' conversion to declared limits follows
pub(super) fn over_limit(ctx: &mut Ctx, chart: &str, what: &str, count: usize, path: Option<&str>) -> String {
    let max = crate::limits::max(chart, what);
    let noun = chart.replace('_', " ");
    ctx.check(
        "limit-exceeded",
        Level::Error,
        format!("{count} {what} exceed the limit of {max} {what} of a {noun} chart"),
        Some("Filter or aggregate the rows to fit the limit, or choose a chart type that takes more."),
        path.map(str::to_owned),
        None,
        json!({"chart": chart, "what": what, "count": count, "limit": max}),
    );
    format!("Cannot draw this {noun} chart · {count} {what} exceed the limit of {max}")
}

/// Every panel failed: the chart is its first reason, in a frame of the usual ratio.
pub(super) fn failed(ctx: &Ctx, reason: &str, ratio: f64) -> Painted {
    let width = ctx.opts.width;
    let h = (width / ratio).max(MIN_PLOT);
    Painted {
        height: h,
        panels: vec![super::failed_panel(ctx, Rect::new(0.0, 0.0, width, h), reason)],
        figure_items: Vec::new(),
        legend: None,
        failure: Some(reason.to_owned()),
    }
}

// ---------------------------------------------------------------------------------------------
// Charts with a calendar or numeric x axis (band, density, ecdf and the like).

/// How an xy chart reads its x field.
pub(super) enum XAxis {
    Calendar {
        extent: [f64; 2],
        precision: crate::time::Precision,
        spacing: i64,
    },
    Numeric {
        extent: [f64; 2],
    },
}

impl XAxis {
    pub fn extent(&self) -> [f64; 2] {
        match self {
            XAxis::Calendar { extent, .. } | XAxis::Numeric { extent } => *extent,
        }
    }
}

/// Read the x field of `members`: dates when every value is an ISO date, numbers when every
/// value is a number, otherwise the reason it cannot be drawn. The result holds one entry per
/// row of the chart, `None` for rows outside `members`.
pub(super) fn read_x(ctx: &Ctx, members: &[usize], field: &str) -> Result<(Vec<Option<f64>>, XAxis), String> {
    let mut xs: Vec<Option<f64>> = vec![None; ctx.rows.len()];
    let dates: Option<Vec<(i64, crate::time::Precision)>> = members
        .iter()
        .map(|&i| {
            crate::rows::cell(&ctx.rows[i], field)
                .as_str()
                .and_then(crate::time::parse_iso)
        })
        .collect();
    let axis = if let Some(d) = dates.filter(|d| !d.is_empty()) {
        let mut sorted: Vec<i64> = d.iter().map(|(ms, _)| *ms).collect();
        sorted.sort_unstable();
        sorted.dedup();
        let mut gaps: Vec<i64> = sorted.windows(2).map(|w| w[1] - w[0]).collect();
        gaps.sort_unstable();
        for (&i, (ms, _)) in members.iter().zip(&d) {
            xs[i] = Some(*ms as f64);
        }
        XAxis::Calendar {
            extent: [sorted[0] as f64, *sorted.last().unwrap_or(&sorted[0]) as f64],
            precision: d.iter().map(|(_, p)| *p).max().unwrap_or(crate::time::Precision::Day),
            spacing: gaps.get(gaps.len() / 2).copied().unwrap_or(crate::time::MS_DAY),
        }
    } else {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for &i in members {
            match crate::rows::num(&ctx.rows[i], field) {
                crate::rows::Num::Value(v) => {
                    xs[i] = Some(v);
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                crate::rows::Num::Missing => {}
                _ => return Err(format!("{field} must hold dates or numbers")),
            }
        }
        if !lo.is_finite() {
            return Err(format!("{field} has no values"));
        }
        XAxis::Numeric { extent: [lo, hi] }
    };
    Ok((xs, axis))
}

/// The part of `whole` a zoom `window` shows: the window clipped to the whole, when that is a
/// narrower one (FEA-CH-49). A window that is not a pair of numbers, or leaves nothing of the
/// whole, shows all of it.
pub(super) fn zoom_window(window: Option<[f64; 2]>, whole: [f64; 2]) -> Option<[f64; 2]> {
    let [a, b] = window?;
    if !(a.is_finite() && b.is_finite()) {
        return None;
    }
    let shown = [a.max(whole[0]), b.min(whole[1])];
    (shown[1] > shown[0] && (shown[0] > whole[0] || shown[1] < whole[1])).then_some(shown)
}

/// One panel of an xy chart: the plot, the y axis and the map from x values to pixels.
pub(super) struct XyFrame {
    pub plot: Rect,
    pub value: ValueAxis,
    domain: [f64; 2],
    pub scale: crate::geometry::XScale,
    /// Grid, labels and titles in paint order; marks go after them.
    pub items: Vec<Item>,
}

impl XyFrame {
    /// Pixel position of x value `v`.
    pub fn x(&self, v: f64) -> f64 {
        let [a, b] = self.domain;
        self.plot.x + crate::scale::position(v, a, b) * self.plot.w
    }
    /// Pixel position of y value `v`.
    pub fn y(&self, v: f64) -> f64 {
        self.value.map(v, self.plot.bottom(), self.plot.y)
    }
}

/// Draw the frame of an xy panel inside `outer`: y axis with grid and baseline, x labels on
/// calendar or round ticks, the axis titles, and the zoom window the runtime asked for.
pub(super) fn xy_frame(
    ctx: &mut Ctx,
    plan: &Plan,
    outer: &Rect,
    x: &XAxis,
    x_title: Option<&str>,
    side: &ValueSide,
) -> XyFrame {
    let lh = ctx.line_height();
    let size = ctx.size();
    let font = ctx.style.font.clone();
    let (ascent, _) = font.vertical();
    let top = outer.y + plan.title_room + lh + 4.0 + lh / 2.0;
    let bottom_room = lh + LABEL_GAP + if x_title.is_some() { lh } else { 0.0 };
    let plot_h = (outer.bottom() - top - bottom_room).max(MIN_PLOT * 0.5);
    // A chart that zooms each axis on its own window (FEA-CH-49) draws the window along y the
    // runtime asked for, inside the axis it fixes.
    let whole_y = match (side.min, side.max) {
        (Some(lo), Some(hi)) if lo < hi && crate::interaction::zooms_each_axis(ctx.block.kind.name()) => Some([lo, hi]),
        _ => None,
    };
    let window = whole_y.and_then(|whole| zoom_window(ctx.opts.zoom_y, whole));
    let value = axis::value_axis(&ValueSpec {
        data: window.unwrap_or(side.data),
        zero: side.zero,
        min: window.map_or(side.min, |w| Some(w[0])),
        max: window.map_or(side.max, |w| Some(w[1])),
        length: plot_h,
        horizontal: false,
        format: side.format,
        font: &ctx.style.figures,
        size,
    });
    let left = outer.x + value.widest(&ctx.style.figures, size) + LABEL_GAP;
    let plot = Rect::new(left, top, outer.right() - left - 12.0, plot_h);
    let mut items = Vec::new();
    grid(ctx, plot, &value, false, &mut items);
    let ink = ctx.style.ink;
    items.push(Item::Text(ctx.text(
        &side.title,
        outer.x,
        top - lh / 2.0 - 6.0,
        Anchor::Start,
        Face::Data,
        Role::AxisTitle,
        ink,
    )));
    let extent = x.extent();
    let (domain, ticks, calendar) = x_scale(ctx, x, &plot);
    for (px, label) in &ticks {
        // The first and last labels stay inside the figure; they lean off their tick if need be.
        let half = ctx.style.figures.width(label, size) / 2.0;
        let at = px.clamp(outer.x + half, (outer.right() - half).max(outer.x + half));
        items.push(Item::Text(ctx.text(
            label,
            at,
            plot.bottom() + LABEL_GAP + ascent * size,
            Anchor::Middle,
            Face::Figures,
            Role::AxisLabel,
            ink,
        )));
    }
    if let Some(title) = x_title {
        items.push(Item::Text(ctx.text(
            title,
            plot.right(),
            plot.bottom() + LABEL_GAP + lh + ascent * size,
            Anchor::End,
            Face::Data,
            Role::AxisTitle,
            ink,
        )));
    }
    let y = whole_y.map(|extent| crate::geometry::YSpan {
        domain: [value.lo, value.hi],
        extent,
        range: [plot.bottom(), plot.y],
    });
    XyFrame {
        plot,
        value,
        domain,
        scale: crate::geometry::XScale {
            domain,
            extent,
            range: [plot.x, plot.right()],
            calendar,
            y,
            least: None,
        },
        items,
    }
}

/// The visible domain of an x axis drawn along `plot` (the zoom window the runtime asked for, or
/// all data), its tick positions in pixels with their labels, and whether it is a calendar.
pub(super) fn x_scale(ctx: &Ctx, x: &XAxis, plot: &Rect) -> ([f64; 2], Vec<(f64, String)>, bool) {
    let font = ctx.style.font.clone();
    let size = ctx.size();
    let extent = x.extent();
    let window = match ctx.opts.zoom {
        Some([a, b]) if a < b => [a.max(extent[0]), b.min(extent[1])],
        _ => extent,
    };
    let (domain, ticks, calendar): ([f64; 2], Vec<(f64, String)>, bool) = match x {
        XAxis::Calendar { precision, spacing, .. } => {
            let [a, b] = window;
            let b = if b > a { b } else { a + crate::time::MS_DAY as f64 };
            let cal = axis::calendar_axis(
                a as i64,
                b as i64,
                *spacing,
                *precision,
                plot.w,
                &font,
                size,
                ctx.calendar_locale(),
            );
            let pos = |v: f64| plot.x + crate::scale::position(v, a, b) * plot.w;
            (
                [a, b],
                cal.ticks
                    .iter()
                    .zip(&cal.labels)
                    .map(|(t, l)| (pos(*t as f64), l.clone()))
                    .collect(),
                true,
            )
        }
        XAxis::Numeric { .. } => {
            let [a, b] = window;
            let b = if b > a { b } else { a + a.abs().max(1.0) * 1e-9 };
            let format = Format {
                kind: ctx.block.x.format.unwrap_or(crate::numbers::Kind::Auto),
                unit: ctx.block.x.unit.clone(),
                digits: ctx.block.x.digits.unwrap_or(ctx.block.digits),
                locale: ctx.locale,
            };
            let ax = axis::value_axis(&ValueSpec {
                data: [a, b],
                zero: Zero::None,
                min: Some(a),
                max: Some(b),
                length: plot.w,
                horizontal: true,
                format: &format,
                font: &ctx.style.figures,
                size,
            });
            let pos = |v: f64| plot.x + crate::scale::position(v, ax.lo, ax.hi) * plot.w;
            (
                [ax.lo, ax.hi],
                ax.ticks
                    .iter()
                    .zip(&ax.labels)
                    .map(|(t, l)| (pos(*t), l.clone()))
                    .collect(),
                false,
            )
        }
    };
    (domain, ticks, calendar)
}

/// An xy panel: marks over the frame, nearest-x targeting, and the facet title.
pub(super) fn xy_panel(
    ctx: &Ctx,
    outer: &Rect,
    frame: XyFrame,
    marks: Vec<Item>,
    observations: Vec<Observation>,
    name: &Option<String>,
) -> Panel {
    let mut items = frame.items;
    items.extend(marks);
    let mut panel = Panel {
        facet: None,
        frame: *outer,
        plot: frame.plot,
        items,
        observations,
        targeting: Targeting::NearestX,
        x: Some(frame.scale),
        failure: None,
    };
    super::panel_title(ctx, &mut panel, name);
    panel
}

impl Plan {
    /// A plan for xy charts: no categories, one panel size from the ratio.
    pub fn xy(ctx: &Ctx, panels: usize, default_ratio: f64) -> Plan {
        let mut plan = plan(ctx, panels, &[], 1.0, default_ratio);
        plan.horizontal = false;
        plan
    }
}
