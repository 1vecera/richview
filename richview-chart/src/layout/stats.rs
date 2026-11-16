//! Charts that compute what they draw, and interval charts: histograms, box plots and authored
//! intervals (FEA-CH-11 to FEA-CH-14, FEA-CH-70).

use super::{BASELINE_WIDTH, Ctx, GRID_WIDTH, LABEL_GAP, MIN_PLOT, POINT_RADIUS, Painted, Style};
use crate::axis::{self, CategoryLayout, ValueAxis, ValueSpec, Zero};
use crate::binning;
use crate::block::{ChartType, Measure, Orientation};
use crate::box_summary;
use crate::derived::Position as BoxCoordinate;
use crate::diag::Level;
use crate::geometry::{
    Anchor, Face, Fill, Item, Legend, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke,
    Swatch, Targeting, XScale,
};
use crate::numbers::{self, Format, Kind};
use crate::payload::Quant;
use crate::rows::{self, Num};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Most lines a horizontal category name wraps to (FEA-CH-26).
const NAME_LINES: usize = 4;

pub(super) fn layout(ctx: &mut Ctx, facets: &[(Option<String>, Vec<usize>)]) -> Painted {
    match ctx.block.kind {
        ChartType::Histogram => histogram(ctx, facets),
        ChartType::Boxplot => grouped(ctx, facets, Kind2::Box),
        _ => grouped(ctx, facets, Kind2::Interval),
    }
}

/// Reads a numeric channel for every row, disclosing what cannot be used.
struct Column {
    values: Vec<(usize, f64)>,
    missing: usize,
    invalid: Vec<(usize, String)>,
    underflow: Vec<(usize, String)>,
}

fn column(ctx: &Ctx, field: &str, members: &[usize]) -> Column {
    let mut out = Column {
        values: Vec::new(),
        missing: 0,
        invalid: Vec::new(),
        underflow: Vec::new(),
    };
    for &i in members {
        match rows::num(&ctx.rows[i], field) {
            Num::Value(v) => out.values.push((i, v)),
            Num::Missing => out.missing += 1,
            Num::Invalid(t) => out.invalid.push((i, t)),
            Num::Underflow(t) => out.underflow.push((i, t)),
        }
    }
    out
}

fn panel_height(ctx: &Ctx, width: f64, title_room: f64, ratio_default: f64) -> f64 {
    let ratio = ctx.block.style.ratio.unwrap_or(ratio_default);
    (width / ratio + title_room).max(MIN_PLOT + 3.0 * ctx.line_height() + title_room)
}

fn legend_for(ctx: &Ctx, swatch: Swatch) -> Option<Legend> {
    (ctx.block.encoding.color.is_some() && ctx.series.len() > 1).then(|| Legend {
        swatch,
        entries: (0..ctx.series.len()).collect(),
        bottom: ctx.style.legend_bottom,
    })
}

/// Value gridlines and labels along a vertical value axis.
fn value_grid(ctx: &Ctx, plot: Rect, axis: &ValueAxis, horizontal: bool, items: &mut Vec<Item>) {
    let ink = ctx.style.ink;
    let (ascent, _) = ctx.style.font.vertical();
    for (t, label) in axis.ticks.iter().zip(&axis.labels) {
        if horizontal {
            let x = axis.map(*t, plot.x, plot.right());
            if *t != 0.0 {
                items.push(Item::Line {
                    path: Path::polyline(&[[x, plot.y], [x, plot.bottom()]]),
                    stroke: Stroke::solid(Style::rgba(ctx.style.rule_soft), GRID_WIDTH),
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

// ---------------------------------------------------------------------------------------------
// Histograms.

fn histogram(ctx: &mut Ctx, facets: &[(Option<String>, Vec<usize>)]) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let keys: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![block.x.label.clone().unwrap_or_else(|| x_field.clone())],
    };
    ctx.set_series(&keys);
    let width = ctx.opts.width;
    let facet_of = super::facet_index(facets, ctx.rows.len());
    let all: Vec<usize> = (0..ctx.rows.len()).filter(|&r| facet_of[r].is_some()).collect();
    let col = column(ctx, &x_field, &all);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    for (row, text) in &col.invalid {
        if let Some(f) = facet_of[*row] {
            failures[f] = Some(format!(
                "Cannot draw this histogram · {x_field} has text that is not a number"
            ));
        }
        ctx.check(
            "non-numeric",
            Level::Error,
            format!("Row {} has {x_field} '{text}', which is not a number", row + 1),
            Some("Write plain numbers; missing values may be empty or NA."),
            Some(Ctx::row_path(*row, &x_field)),
            None,
            json!({"row": row}),
        );
    }
    for (row, text) in &col.underflow {
        if let Some(f) = facet_of[*row] {
            failures[f].get_or_insert_with(|| {
                format!("Cannot draw this histogram · {x_field} has a value too small to represent")
            });
        }
        ctx.underflow(*row, &x_field, text);
    }
    // The whole chart fails when no panel can draw; otherwise a bad panel fails alone.
    if failures.iter().all(Option::is_some) {
        let reason = failures
            .iter()
            .flatten()
            .next()
            .cloned()
            .unwrap_or_else(|| format!("Cannot draw this histogram · {x_field} has no values to draw"));
        let panel = super::failed_panel(ctx, Rect::new(0.0, 0.0, width, width / 1.78), &reason);
        return Painted {
            height: width / 1.78,
            panels: vec![panel],
            figure_items: vec![],
            legend: None,
            failure: Some(reason),
        };
    }
    // Edges resolve over the whole dataset before facets (TYP-20).
    let values: Vec<f64> = col.values.iter().map(|(_, v)| *v).collect();
    let spec = binning::Spec {
        count: block.binning.count,
        edges: block.binning.edges.clone(),
        extent: block.binning.extent,
        measure: binning::Measure::Count,
    };
    // The edges and the tallies jump with the data (a value on an edge, Sturges' count), so the
    // exact rows decide them (TYP-36).
    let edge_format = block.label_format(&block.x, ctx.locale);
    let resolve = || spec.resolve_shown(&values, &edge_format).map_err(str::to_owned);
    // Authored boundaries already travel in the block; replay them verbatim instead of
    // passing them through the derived-coordinate precision cap.
    let resolved: Result<Vec<f64>, String> = if spec.edges.is_some() {
        resolve()
    } else {
        ctx.derive_projected(0, "histogram.edges", resolve)
    };
    let edges = match resolved {
        Ok(e) => e,
        Err(message) => {
            ctx.check(
                "histogram",
                Level::Error,
                message,
                Some("Give binning a count from 1 to 256, an increasing extent, or increasing edges."),
                Some("chart.binning".into()),
                None,
                Value::Null,
            );
            let reason = "Cannot draw this histogram · Invalid binning".to_owned();
            let panel = super::failed_panel(ctx, Rect::new(0.0, 0.0, width, width / 1.78), &reason);
            return Painted {
                height: width / 1.78,
                panels: vec![panel],
                figure_items: vec![],
                legend: None,
                failure: Some(reason),
            };
        }
    };
    let bins = edges.len() - 1;
    let measure = block.binning.measure;
    let measure_name = match measure {
        Measure::Count => "Count",
        Measure::Proportion => "Share",
        Measure::Density => "Density",
    };
    let group_of = |row: usize| -> usize {
        match &block.encoding.color {
            Some(f) => rows::text(&ctx.rows[row], f)
                .and_then(|k| keys.iter().position(|x| *x == k))
                .unwrap_or(0),
            None => 0,
        }
    };
    // Per panel, per bin, per group counts, the panel's total and the values outside the bins.
    let counted: Vec<(Vec<Vec<usize>>, usize, usize)> = (0..facets.len())
        .map(|fi| {
            ctx.derive(fi, "histogram.tally", || {
                let mut tally = vec![vec![0usize; keys.len()]; bins];
                let (mut total, mut outside) = (0usize, 0usize);
                for &(row, v) in &col.values {
                    if facet_of[row] != Some(fi) || failures[fi].is_some() {
                        continue;
                    }
                    let bin = if spec.extent.is_some_and(|[lo, hi]| v < lo || v > hi) {
                        None
                    } else {
                        binning::bin_index(&edges, v)
                    };
                    match bin {
                        Some(b) => {
                            tally[b][group_of(row)] += 1;
                            total += 1;
                        }
                        None => outside += 1,
                    }
                }
                (tally, total, outside)
            })
        })
        .collect();
    let outside: usize = counted.iter().map(|c| c.2).sum();
    let totals: Vec<usize> = counted.iter().map(|c| c.1).collect();
    let tallies: Vec<Vec<Vec<usize>>> = counted.into_iter().map(|c| c.0).collect();
    let visible_counts: Vec<Vec<usize>> = tallies
        .iter()
        .map(|bins| {
            bins.iter()
                .map(|groups| {
                    groups
                        .iter()
                        .enumerate()
                        .filter(|(g, _)| !ctx.is_hidden(*g))
                        .map(|(_, count)| count)
                        .sum()
                })
                .collect()
        })
        .collect();
    let measure_of = |count: usize, total: usize, b: usize| -> f64 {
        let w = edges[b + 1] - edges[b];
        match measure {
            Measure::Count => count as f64,
            Measure::Proportion => count as f64 / total.max(1) as f64,
            Measure::Density => count as f64 / total.max(1) as f64 / w,
        }
    };
    let mut y_hi = 0.0f64;
    for (counts, total) in visible_counts.iter().zip(&totals) {
        for (b, count) in counts.iter().enumerate() {
            y_hi = y_hi.max(measure_of(*count, *total, b));
        }
    }
    // A playback keeps one value axis for every frame (FEA-BR-31).
    if let Some([_, hi]) = ctx.wanted().y {
        y_hi = y_hi.max(hi);
    }
    ctx.fit.y = Some([0.0, y_hi]);
    let excluded = col.missing + outside;
    if excluded > 0 {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{excluded} {} not counted: {} missing, {} outside the bins",
                if excluded == 1 { "row is" } else { "rows are" },
                col.missing,
                outside
            ),
            None,
            Some("chart.encoding.x".into()),
            None,
            json!({"missing": col.missing, "outside": outside}),
        );
    }
    let n: usize = visible_counts.iter().flatten().sum();
    let count_adjustment = block
        .binning
        .count
        .filter(|requested| *requested != bins)
        .map(|requested| format!("; requested {requested}, adjusted for the chosen precision"))
        .unwrap_or_default();
    ctx.check(
        "binning",
        Level::Info,
        format!(
            "{n} observations in {bins} equal-width intervals ({}{count_adjustment})",
            if block.binning.count.is_some() || block.binning.edges.is_some() {
                "authored"
            } else {
                "Sturges' rule"
            }
        ),
        None,
        Some("chart.binning".into()),
        None,
        json!({"intervals": bins, "edges": edges}),
    );
    ctx.note.push(format!(
        "{n} observations in {bins} intervals{}",
        if excluded > 0 {
            format!(" · {excluded} not counted")
        } else {
            String::new()
        }
    ));
    let title_room = if block.encoding.facet.is_some() {
        ctx.line_height() + 6.0
    } else {
        0.0
    };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let ph = panel_height(ctx, pw, title_room, 16.0 / 9.0);
    let frames = super::grid_frames(facets.len(), width, 0.0, ph);
    let x_format = Format {
        kind: block.x.format.unwrap_or(Kind::Auto),
        unit: block.x.unit.clone(),
        digits: block.x.digits.unwrap_or(block.digits),
        locale: ctx.locale,
    };
    let y_format = match measure {
        Measure::Proportion => Format {
            kind: Kind::Percent,
            unit: None,
            digits: block.digits,
            locale: ctx.locale,
        },
        _ => block.label_format(&block.y, ctx.locale),
    };
    let readout_x = block.readout_format(&block.x, ctx.locale);
    let authored_edges = spec.edges.is_some() || spec.extent.is_some();
    let edge_readout = Format {
        digits: if authored_edges { 17 } else { readout_x.digits },
        ..readout_x.clone()
    };
    let edge_summary = Format {
        digits: if authored_edges { 17 } else { x_format.digits },
        ..x_format.clone()
    };
    let readout_y = Format {
        digits: block.readout_digits,
        ..y_format.clone()
    };
    let lh = ctx.line_height();
    let size = ctx.size();
    let font = ctx.style.font.clone();
    let mut panels = Vec::new();
    for ((((name, _), frame), (t, total)), failure) in facets
        .iter()
        .zip(&frames)
        .zip(tallies.iter().zip(&totals))
        .zip(&failures)
    {
        if let Some(reason) = failure {
            panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
            continue;
        }
        let top = frame.y + title_room + lh + 4.0 + lh / 2.0;
        let plot_h = (frame.bottom() - top - lh - LABEL_GAP - if block.x.label.is_some() { lh } else { 0.0 })
            .max(MIN_PLOT * 0.5);
        let y = axis::value_axis(&ValueSpec {
            data: [0.0, y_hi.max(f64::MIN_POSITIVE)],
            zero: Zero::Include,
            min: block.y.min.is_some().then_some(0.0),
            max: block.y.max,
            length: plot_h,
            horizontal: false,
            format: &y_format,
            font: &ctx.style.figures,
            size,
        });
        let left = frame.x + y.widest(&ctx.style.figures, size) + LABEL_GAP;
        let mut plot = Rect::new(left, top, frame.right() - left - 6.0, plot_h);
        let x_axis = axis::value_axis(&ValueSpec {
            data: [edges[0], edges[bins]],
            zero: Zero::None,
            min: Some(edges[0]),
            max: Some(edges[bins]),
            length: plot.w,
            horizontal: true,
            format: &x_format,
            font: &ctx.style.figures,
            size,
        });
        let last_half = x_axis
            .labels
            .last()
            .map_or(0.0, |l| ctx.style.figures.width(l, size) / 2.0);
        let first_half = x_axis
            .labels
            .first()
            .map_or(0.0, |l| ctx.style.figures.width(l, size) / 2.0);
        if plot.x - frame.x < first_half {
            plot.w -= first_half - (plot.x - frame.x);
            plot.x = frame.x + first_half;
        }
        plot.w = plot.w.min(frame.right() - last_half - plot.x);
        let xmap = |v: f64| plot.x + (v - edges[0]) / (edges[bins] - edges[0]) * plot.w;
        let ymap = |v: f64| y.map(v, plot.bottom(), plot.y);
        let mut items = Vec::new();
        value_grid(ctx, plot, &y, false, &mut items);
        let (ascent, _) = font.vertical();
        for (tick, label) in x_axis.ticks.iter().zip(&x_axis.labels) {
            items.push(Item::Text(ctx.text(
                label,
                xmap(*tick),
                plot.bottom() + LABEL_GAP + ascent * size,
                Anchor::Middle,
                Face::Figures,
                Role::AxisLabel,
                ctx.style.ink,
            )));
        }
        let title = block.y.label.clone().unwrap_or_else(|| measure_name.to_owned());
        items.push(Item::Text(ctx.text(
            &title,
            frame.x,
            top - lh / 2.0 - 6.0,
            Anchor::Start,
            Face::Data,
            Role::AxisTitle,
            ctx.style.ink,
        )));
        if let Some(label) = &block.x.label {
            items.push(Item::Text(ctx.text(
                label,
                plot.right(),
                plot.bottom() + LABEL_GAP + lh + ascent * size,
                Anchor::End,
                Face::Data,
                Role::AxisTitle,
                ctx.style.ink,
            )));
        }
        let mut observations = Vec::new();
        for (b, groups) in t.iter().enumerate() {
            let (x0, x1) = (xmap(edges[b]), xmap(edges[b + 1]));
            let mut stacked = 0usize;
            let mut lines = Vec::new();
            for (g, count) in groups.iter().enumerate() {
                if ctx.is_hidden(g) || *count == 0 {
                    continue;
                }
                let from = measure_of(stacked, *total, b);
                stacked += count;
                let to = measure_of(stacked, *total, b);
                // Histogram intervals touch: fixed pixel gaps or baseline insets change density
                // area, especially when the author gives bins of different widths (FEA-CH-11).
                let rect = Rect::new(x0, ymap(to), (x1 - x0).max(0.0), (ymap(from) - ymap(to)).max(0.0));
                let color = Style::rgba(ctx.series_color(g));
                let hatch = ctx.series[g].hatch;
                items.push(Item::Rect {
                    rect,
                    fill: Fill {
                        color,
                        hatch,
                        hatch_ink: hatch.map(|_| Style::rgba(ctx.style.surface)),
                        hatch_scale: 1.0,
                    },
                    stroke: None,
                    radius: [0.0; 4],
                    role: Role::Mark,
                    series: Some(g),
                    obs: Some(b),
                });
                if keys.len() > 1 {
                    lines.push(ReadoutLine {
                        name: ctx.series[g].name.clone(),
                        value: numbers::value(measure_of(*count, *total, b), &readout_y),
                        series: Some(g),
                        names_series: true,
                        derived: true,
                    });
                }
            }
            let value = measure_of(stacked, *total, b);
            lines.insert(
                0,
                ReadoutLine {
                    name: measure_name.into(),
                    value: numbers::value(value, &readout_y),
                    series: None,
                    names_series: false,
                    derived: true,
                },
            );
            let last = b + 1 == bins;
            let heading = format!(
                "{} to {}{}",
                numbers::value(edges[b], &edge_readout),
                numbers::value(edges[b + 1], &edge_readout),
                if last { " (inclusive)" } else { "" }
            );
            let area = Rect::new(
                x0,
                ymap(value).min(plot.bottom() - 6.0),
                x1 - x0,
                plot.bottom() - ymap(value).min(plot.bottom() - 6.0),
            );
            observations.push(Observation {
                row: None,
                dots: vec![],
                series: None,
                at: [(x0 + x1) / 2.0, ymap(value)],
                wedge: None,
                area: Some(area),
                radius: 0.0,
                x: (x0 + x1) / 2.0,
                readout: Readout { heading, lines }.into(),
            });
        }
        let mut panel = Panel {
            facet: None,
            frame: *frame,
            plot,
            items,
            observations,
            targeting: Targeting::Contain,
            x: Some(XScale {
                domain: [edges[0], edges[bins]],
                extent: [edges[0], edges[bins]],
                range: [plot.x, plot.right()],
                calendar: false,
            }),
            failure: None,
        };
        super::panel_title(ctx, &mut panel, name);
        panels.push(panel);
    }
    let fullest = visible_counts
        .iter()
        .flat_map(|counts| counts.iter().copied().enumerate())
        .max_by_key(|(_, c)| *c);
    ctx.summary = match fullest {
        Some((b, c)) if c > 0 => format!(
            "{} from {} to {}; the fullest interval, {} to {}, holds {c}.",
            super::count(n, "observation", "observations"),
            numbers::value(edges[0], &edge_summary),
            numbers::value(edges[bins], &edge_summary),
            numbers::value(edges[b], &edge_summary),
            numbers::value(edges[b + 1], &edge_summary)
        ),
        _ => "No observations.".into(),
    };
    let legend = legend_for(ctx, Swatch::Square);
    Painted {
        height: super::grid_height(facets.len(), columns, ph, super::FACET_GAP_Y),
        panels,
        figure_items: vec![],
        legend,
        failure: None,
    }
}

// ---------------------------------------------------------------------------------------------
// Box plots and intervals: values per category and group.

#[derive(Clone, Copy, PartialEq)]
enum Kind2 {
    Box,
    Interval,
}

#[derive(Serialize, Deserialize)]
struct BoxTexts {
    lower_whisker: String,
    q1: String,
    median: String,
    q3: String,
    upper_whisker: String,
    median_label: String,
}

/// Numeric statistics are grid indices; reader text is formatted from the exact statistics.
#[derive(Serialize, Deserialize)]
struct BoxRecord {
    count: usize,
    min: BoxCoordinate,
    q1: BoxCoordinate,
    median: BoxCoordinate,
    q3: BoxCoordinate,
    max: BoxCoordinate,
    lower_whisker: BoxCoordinate,
    upper_whisker: BoxCoordinate,
    outliers: Vec<usize>,
    text: BoxTexts,
}

impl BoxRecord {
    fn encode(s: box_summary::Summary, grid: Option<&Quant>, readout: &Format, label: &Format) -> Self {
        let text = BoxTexts {
            lower_whisker: numbers::value(s.lower_whisker, readout),
            q1: numbers::value(s.q1, readout),
            median: numbers::value(s.median, readout),
            q3: numbers::value(s.q3, readout),
            upper_whisker: numbers::value(s.upper_whisker, readout),
            median_label: numbers::value(s.median, label),
        };
        Self {
            count: s.count,
            min: BoxCoordinate::encode(s.min, grid, readout),
            q1: BoxCoordinate::encode(s.q1, grid, readout),
            median: BoxCoordinate::encode(s.median, grid, readout),
            q3: BoxCoordinate::encode(s.q3, grid, readout),
            max: BoxCoordinate::encode(s.max, grid, readout),
            lower_whisker: BoxCoordinate::encode(s.lower_whisker, grid, readout),
            upper_whisker: BoxCoordinate::encode(s.upper_whisker, grid, readout),
            outliers: s.outliers,
            text,
        }
    }

    fn decode(self, grid: Option<&Quant>) -> (box_summary::Summary, BoxTexts) {
        (
            box_summary::Summary {
                count: self.count,
                min: self.min.at(grid),
                q1: self.q1.at(grid),
                median: self.median.at(grid),
                q3: self.q3.at(grid),
                max: self.max.at(grid),
                lower_whisker: self.lower_whisker.at(grid),
                upper_whisker: self.upper_whisker.at(grid),
                outliers: self.outliers,
            },
            self.text,
        )
    }
}

#[derive(Serialize, Deserialize)]
struct BoxRecords {
    grid: Option<Quant>,
    groups: Vec<Option<BoxRecord>>,
}

struct Slot {
    /// Facet panel; slots never span panels (FEA-CH-35).
    facet: usize,
    category: usize,
    group: usize,
    rows: Vec<usize>,
    /// Box: the summary; interval: lower, estimate, upper.
    summary: Option<box_summary::Summary>,
    box_texts: Option<BoxTexts>,
    samples: Vec<(usize, f64)>,
    interval: Option<(f64, Option<f64>, f64)>,
}

fn grouped(ctx: &mut Ctx, facets: &[(Option<String>, Vec<usize>)], kind: Kind2) -> Painted {
    let block = ctx.block;
    let width = ctx.opts.width;
    let x_field = block.encoding.x.clone();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let categories: Vec<String> = match &x_field {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![block.y.label.clone().unwrap_or_else(|| y_field.clone())],
    };
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![block.y.label.clone().unwrap_or_else(|| y_field.clone())],
    };
    ctx.set_series(&groups);
    let multi = block.encoding.color.is_some();
    let readout = block.readout_format(&block.y, ctx.locale);
    let label_format = block.label_format(&block.y, ctx.locale);
    let mut excluded = 0usize;
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let facet_of = super::facet_index(facets, ctx.rows.len());
    let mut slots: Vec<Slot> = Vec::new();
    for (i, row) in ctx.rows.iter().enumerate() {
        // Rows in no facet panel are disclosed once by the layout.
        let Some(facet) = facet_of[i] else { continue };
        let category = match &x_field {
            Some(f) => match rows::text(row, f) {
                Some(c) => categories.iter().position(|x| *x == c).unwrap_or(0),
                None => {
                    excluded += 1;
                    continue;
                }
            },
            None => 0,
        };
        let group = match &block.encoding.color {
            Some(f) => match rows::text(row, f) {
                Some(g) => groups.iter().position(|x| *x == g).unwrap_or(0),
                None => {
                    excluded += 1;
                    continue;
                }
            },
            None => 0,
        };
        let slot = match slots
            .iter()
            .position(|s| s.facet == facet && s.category == category && s.group == group)
        {
            Some(s) => s,
            None => {
                slots.push(Slot {
                    facet,
                    category,
                    group,
                    rows: Vec::new(),
                    summary: None,
                    box_texts: None,
                    samples: Vec::new(),
                    interval: None,
                });
                slots.len() - 1
            }
        };
        slots[slot].rows.push(i);
    }
    let mut clipped = 0usize;
    for slot in &mut slots {
        match kind {
            Kind2::Box => {
                for &r in &slot.rows {
                    match rows::num(&ctx.rows[r], &y_field) {
                        Num::Value(v) => slot.samples.push((r, v)),
                        Num::Missing => excluded += 1,
                        Num::Underflow(t) => {
                            ctx.underflow(r, &y_field, &t);
                            failures[slot.facet].get_or_insert_with(|| {
                                format!("Cannot draw this box plot · {y_field} has a value too small to represent")
                            });
                        }
                        Num::Invalid(t) => {
                            ctx.checks.push(crate::geometry::Check {
                                kind: "non-numeric".into(),
                                level: Level::Error,
                                message: format!("Row {} has {y_field} '{t}', which is not a number", r + 1),
                                fix: Some("Write plain numbers; missing values may be empty or NA.".into()),
                                path: Some(Ctx::row_path(r, &y_field)),
                                region: None,
                                facet: None,
                                details: json!({"row": r}),
                            });
                            failures[slot.facet].get_or_insert_with(|| {
                                format!("Cannot draw this box plot · {y_field} has text that is not a number")
                            });
                        }
                    }
                }
            }
            Kind2::Interval => {
                if slot.rows.len() > 1 {
                    let names = (categories[slot.category].clone(), groups[slot.group].clone());
                    ctx.checks.push(crate::geometry::Check {
                        kind: "source-error".into(),
                        level: Level::Error,
                        message: format!(
                            "'{}' / '{}' has {} rows; an interval needs one",
                            names.0,
                            names.1,
                            slot.rows.len()
                        ),
                        fix: Some("Give each category and group one row.".into()),
                        path: Some("chart.encoding.x".into()),
                        region: None,
                        facet: None,
                        details: json!({"rows": slot.rows}),
                    });
                    failures[slot.facet]
                        .get_or_insert_with(|| "Cannot draw these intervals · Repeated categories".into());
                    continue;
                }
                let r = slot.rows[0];
                let lo_field = block.encoding.lower.clone().unwrap_or_default();
                let hi_field = block.encoding.upper.clone().unwrap_or_default();
                // Bounds and the estimate alike (FEA-CH-70).
                let mut too_small = false;
                for field in [Some(&lo_field), Some(&hi_field), block.encoding.y.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    if let Num::Underflow(t) = rows::num(&ctx.rows[r], field) {
                        ctx.underflow(r, field, &t);
                        too_small = true;
                    }
                }
                if too_small {
                    failures[slot.facet].get_or_insert_with(|| {
                        "Cannot draw these intervals · A value is too small to represent".into()
                    });
                    continue;
                }
                let read = |f: &str| match rows::num(&ctx.rows[r], f) {
                    Num::Value(v) => Some(v),
                    _ => None,
                };
                match (read(&lo_field), read(&hi_field)) {
                    (Some(lo), Some(hi)) => {
                        if lo > hi || decimal_greater(&ctx.rows[r], &lo_field, &hi_field) {
                            ctx.checks.push(crate::geometry::Check {
                                kind: "source-error".into(),
                                level: Level::Error,
                                message: format!("Row {}: the lower bound is above the upper bound", r + 1),
                                fix: Some("Swap the bounds or fix the data.".into()),
                                path: Some(Ctx::row_path(r, &lo_field)),
                                region: None,
                                facet: None,
                                details: json!({"row": r}),
                            });
                            failures[slot.facet].get_or_insert_with(|| {
                                "Cannot draw these intervals · A lower bound is above its upper bound".into()
                            });
                            continue;
                        }
                        let estimate = block.encoding.y.as_ref().and_then(|f| read(f));
                        if (lo != hi) && lo + (hi - lo) == lo {
                            ctx.check(
                                "collapsed-interval",
                                Level::Warning,
                                format!("Row {} is narrower than the chart can show", r + 1),
                                None,
                                Some(Ctx::row_path(r, &lo_field)),
                                None,
                                json!({"row": r}),
                            );
                        }
                        if block.y.min.is_some_and(|m| lo < m) || block.y.max.is_some_and(|m| hi > m) {
                            clipped += 1;
                        }
                        slot.interval = Some((lo, estimate, hi));
                    }
                    _ => excluded += 1,
                }
            }
        }
    }
    if kind == Kind2::Box {
        // Quartiles, fences and outlier sets jump with the data, so the exact rows decide them
        // (TYP-36).
        for fi in 0..facets.len() {
            let own: Vec<usize> = (0..slots.len()).filter(|&i| slots[i].facet == fi).collect();
            let records: BoxRecords = ctx.derive_projected(fi, "boxplot.summaries", || {
                // Fractional statistics use the general float grid even when observations use
                // an integer-resolution grid. Preserve any supplied finer grid (TYP-36).
                let summaries: Vec<_> = own
                    .iter()
                    .map(|&i| box_summary::summarize(&slots[i].samples).ok())
                    .collect();
                let shown = summaries
                    .iter()
                    .flatten()
                    .flat_map(|s| [s.min, s.q1, s.median, s.q3, s.max, s.lower_whisker, s.upper_whisker])
                    .map(|v| numbers::shown(v, &readout));
                let grid = crate::derived::statistic_grid_with_shown(ctx.grid_for(&y_field), shown);
                let groups = summaries
                    .into_iter()
                    .map(|s| s.map(|s| BoxRecord::encode(s, grid.as_ref(), &readout, &label_format)))
                    .collect();
                BoxRecords { grid, groups }
            });
            for (&i, record) in own.iter().zip(records.groups) {
                if let Some(record) = record {
                    let (summary, text) = record.decode(records.grid.as_ref());
                    slots[i].summary = Some(summary);
                    slots[i].box_texts = Some(text);
                }
            }
        }
    }
    // The whole chart fails when no panel can draw; otherwise a bad panel fails alone.
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        let h = width / 1.78;
        let panel = super::failed_panel(ctx, Rect::new(0.0, 0.0, width, h), &reason);
        return Painted {
            height: h,
            panels: vec![panel],
            figure_items: vec![],
            legend: None,
            failure: Some(reason),
        };
    }
    if excluded > 0 {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} without a category, group or value {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            None,
            None,
            None,
            json!({"count": excluded}),
        );
    }
    if clipped > 0 {
        ctx.check(
            "clipped-interval",
            Level::Warning,
            format!(
                "{} beyond the authored value limits and {} cut at the edge",
                super::count(clipped, "interval extends", "intervals extend"),
                super::verb(clipped, "is", "are")
            ),
            None,
            Some("chart.axes.y".into()),
            None,
            json!({"count": clipped}),
        );
        ctx.note.push(format!(
            "{} {} cut at the axis limits",
            super::count(clipped, "interval", "intervals"),
            super::verb(clipped, "is", "are")
        ));
    }
    let observations_total: usize = slots.iter().map(|s| s.samples.len()).sum();
    if kind == Kind2::Box {
        ctx.note.push(format!(
            "{observations_total} observations · box = middle half, line = median{}",
            if excluded > 0 {
                format!(" · {excluded} excluded")
            } else {
                String::new()
            }
        ));
        ctx.check(
            "boxplot-summary",
            Level::Info,
            "Boxes span R-7 quartiles; whiskers reach the most extreme observations within 1.5 × IQR",
            None,
            None,
            None,
            Value::Null,
        );
    } else if excluded > 0 {
        ctx.note.push(format!(
            "{} {} not shown",
            super::count(excluded, "row", "rows"),
            super::verb(excluded, "is", "are")
        ));
    }
    // Value range over visible slots.
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for s in slots
        .iter()
        .filter(|s| !ctx.is_hidden(s.group) && failures[s.facet].is_none())
    {
        if let Some(sum) = &s.summary {
            lo = lo.min(sum.min);
            hi = hi.max(sum.max);
        }
        if let Some((a, e, b)) = s.interval {
            lo = lo.min(a).min(e.unwrap_or(a));
            hi = hi.max(b).max(e.unwrap_or(b));
        }
    }
    if !lo.is_finite() {
        [lo, hi] = ctx.wanted().y.unwrap_or([0.0, 1.0]);
    } else {
        [lo, hi] = super::Fit::widen([lo, hi], ctx.wanted().y);
    }
    ctx.fit.y = Some([lo, hi]);
    let names: Vec<String> = categories.clone();
    let ordered = axis::ordered(&names, ctx.locale);
    let reader = ordered.as_ref().map_or_else(|| names.clone(), |o| o.text.clone());
    let title_room = if block.encoding.facet.is_some() {
        ctx.line_height() + 6.0
    } else {
        0.0
    };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let horizontal = match block.style.orientation {
        Orientation::Horizontal => true,
        Orientation::Vertical => false,
        Orientation::Auto => {
            ordered.is_none()
                && matches!(
                    axis::category_layout(
                        &reader,
                        None,
                        (pw - 60.0) / names.len().max(1) as f64,
                        &ctx.style.font,
                        ctx.size()
                    ),
                    CategoryLayout::Unfit { .. }
                )
        }
    };
    let lh = ctx.line_height();
    // Horizontal names sit in a gutter of at most 40% of the panel and wrap to up to four
    // lines there; they are never cut (FEA-CH-26).
    let widest = reader
        .iter()
        .map(|n| ctx.style.font.width(n, ctx.size()))
        .fold(0.0, f64::max);
    let gutter_of = move |w: f64| (widest + LABEL_GAP).min(w * 0.4);
    let name_lines = if horizontal {
        let room = (gutter_of(pw) - LABEL_GAP).max(1.0);
        reader
            .iter()
            .map(|n| ctx.style.font.wrap(n, ctx.size(), room, NAME_LINES).len())
            .max()
            .unwrap_or(1)
            .clamp(1, NAME_LINES) as f64
    } else {
        1.0
    };
    let mut ph = panel_height(ctx, pw, title_room, 16.0 / 9.0);
    if horizontal {
        let per_name =
            (lh * 1.6 * (if multi { groups.len() as f64 * 0.7 } else { 1.0 }).max(1.0)).max(name_lines * lh + lh * 0.6);
        ph = ph.max(names.len() as f64 * per_name + 3.0 * lh + title_room);
    }
    let frames = super::grid_frames(facets.len(), width, 0.0, ph);
    let size = ctx.size();
    let font = ctx.style.font.clone();
    let mut panels = Vec::new();
    for (facet, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[facet] {
            panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
            continue;
        }
        let mut items = Vec::new();
        let gutter = if horizontal { gutter_of(frame.w) } else { 0.0 };
        let top = frame.y + title_room + lh + 4.0 + lh / 2.0;
        // Category names wrap before the plot height is set, so wrapped names get their rows.
        let (ascent, _) = font.vertical();
        let estimate = (frame.w - 80.0).max(1.0) / names.len().max(1) as f64;
        let layout = if horizontal {
            CategoryLayout::OneLine
        } else {
            axis::category_layout(&reader, ordered.as_ref(), estimate, &font, size)
        };
        let name_rows = match &layout {
            CategoryLayout::Wrapped(l)
            | CategoryLayout::Thinned { lines: l, .. }
            | CategoryLayout::Unfit { lines: l, .. } => l.iter().map(Vec::len).max().unwrap_or(1).max(1),
            CategoryLayout::OneLine => 1,
        } as f64;
        // Horizontal charts always title their value axis, in a row under its labels.
        let bottom_room = name_rows * lh + LABEL_GAP + if horizontal || block.x.label.is_some() { lh } else { 0.0 };
        let plot_h = (frame.bottom() - top - bottom_room).max(MIN_PLOT * 0.5);
        let value_len = if horizontal { frame.w - gutter - 16.0 } else { plot_h };
        let zero = if kind == Kind2::Box { Zero::None } else { Zero::Soft };
        let value = axis::value_axis(&ValueSpec {
            data: [lo, hi],
            zero,
            min: block.y.min,
            max: block.y.max,
            length: value_len,
            horizontal,
            format: &label_format,
            font: &ctx.style.figures,
            size,
        });
        let plot = if horizontal {
            let first = value
                .labels
                .first()
                .map_or(0.0, |l| ctx.style.figures.width(l, size) / 2.0);
            let last = value
                .labels
                .last()
                .map_or(0.0, |l| ctx.style.figures.width(l, size) / 2.0);
            let left = frame.x + gutter.max(first);
            Rect::new(left, top, frame.right() - left - last.max(4.0), plot_h)
        } else {
            let left = frame.x + value.widest(&ctx.style.figures, size) + LABEL_GAP;
            Rect::new(left, top, frame.right() - left - 6.0, plot_h)
        };
        value_grid(ctx, plot, &value, horizontal, &mut items);
        let title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
        // A unit the tick labels could not carry at any step is named in the title instead.
        let axis_title = match &value.unit_dropped {
            Some(unit) => format!("{title} ({unit})"),
            None => title.clone(),
        };
        if horizontal {
            items.push(Item::Text(ctx.text(
                &axis_title,
                plot.right(),
                plot.bottom() + LABEL_GAP + lh + ascent * size,
                Anchor::End,
                Face::Data,
                Role::AxisTitle,
                ctx.style.ink,
            )));
        } else {
            items.push(Item::Text(ctx.text(
                &axis_title,
                frame.x,
                top - lh / 2.0 - 6.0,
                Anchor::Start,
                Face::Data,
                Role::AxisTitle,
                ctx.style.ink,
            )));
        }
        let n = names.len().max(1);
        let band = if horizontal { plot.h } else { plot.w } / n as f64;
        let vis_groups: Vec<usize> = (0..groups.len()).filter(|g| !ctx.is_hidden(*g)).collect();
        let inner = band * 0.7;
        let thickness = (inner / vis_groups.len().max(1) as f64).min(if kind == Kind2::Box { 56.0 } else { 28.0 });
        let vmap = |v: f64| {
            if horizontal {
                value.map(v, plot.x, plot.right())
            } else {
                value.map(v, plot.bottom(), plot.y)
            }
        };
        // (category position, value position) to a point.
        let pt = |c: f64, v: f64| if horizontal { [vmap(v), c] } else { [c, vmap(v)] };
        let span = |c0: f64, c1: f64, v0: f64, v1: f64| {
            let (a, b) = (vmap(v0), vmap(v1));
            if horizontal {
                Rect::new(a.min(b), c0, (a - b).abs(), c1 - c0)
            } else {
                Rect::new(c0, a.min(b), c1 - c0, (a - b).abs())
            }
        };
        // Category names.
        if x_field.is_some() && name.as_deref() == facets[0].0.as_deref() {
            match &layout {
                CategoryLayout::Thinned { every, .. } => ctx.check(
                    "label-thinned",
                    Level::Info,
                    format!("Category names show every {every}th"),
                    None,
                    Some("chart.encoding.x".into()),
                    None,
                    json!({"every": every}),
                ),
                CategoryLayout::Unfit { unfit, .. } => {
                    let unfit_names: Vec<&String> = unfit.iter().map(|&i| &reader[i]).collect();
                    ctx.check(
                        "label-unfit",
                        Level::Warning,
                        format!(
                            "{} category names do not fit their space: {}",
                            unfit_names.len(),
                            unfit_names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        ),
                        Some("Shorten the names, or set style.orientation: horizontal."),
                        Some("chart.encoding.x".into()),
                        None,
                        json!({"names": unfit_names}),
                    );
                }
                _ => {}
            }
        }
        let room = (gutter - LABEL_GAP).max(1.0);
        let lines: Vec<Vec<String>> = match layout {
            CategoryLayout::OneLine if horizontal => {
                let wrapped: Vec<Vec<String>> = reader.iter().map(|r| font.wrap(r, size, room, NAME_LINES)).collect();
                let unfit: Vec<&String> = reader
                    .iter()
                    .zip(&wrapped)
                    .filter(|(_, l)| l.len() > NAME_LINES || l.iter().any(|line| font.width(line, size) > room + 0.5))
                    .map(|(r, _)| r)
                    .collect();
                if !unfit.is_empty() && facet == 0 {
                    ctx.check(
                        "label-unfit",
                        Level::Warning,
                        format!(
                            "{} category names do not fit their space: {}",
                            unfit.len(),
                            unfit.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        ),
                        Some("Shorten the names."),
                        Some("chart.encoding.x".into()),
                        None,
                        json!({"names": unfit}),
                    );
                }
                wrapped
            }
            CategoryLayout::OneLine => reader.iter().map(|r| vec![r.clone()]).collect(),
            CategoryLayout::Wrapped(l)
            | CategoryLayout::Thinned { lines: l, .. }
            | CategoryLayout::Unfit { lines: l, .. } => l,
        };
        if x_field.is_some() {
            for (c, l) in lines.iter().enumerate() {
                let center = if horizontal { plot.y } else { plot.x } + (c as f64 + 0.5) * band;
                for (k, line) in l.iter().enumerate() {
                    let t = if horizontal {
                        ctx.text_mid(
                            line,
                            plot.x - LABEL_GAP,
                            center + (k as f64 - (l.len() as f64 - 1.0) / 2.0) * lh,
                            Anchor::End,
                            Face::Data,
                            Role::AxisLabel,
                            ctx.style.ink,
                        )
                    } else {
                        ctx.text(
                            line,
                            center,
                            plot.bottom() + LABEL_GAP + ascent * size + k as f64 * lh,
                            Anchor::Middle,
                            Face::Data,
                            Role::AxisLabel,
                            ctx.style.ink,
                        )
                    };
                    items.push(Item::Text(t));
                }
            }
        }
        let mut observations = Vec::new();
        for s in slots.iter() {
            if ctx.is_hidden(s.group) || s.facet != facet {
                continue;
            }
            let slot_index = vis_groups.iter().position(|g| *g == s.group).unwrap_or(0);
            let center = if horizontal { plot.y } else { plot.x } + (s.category as f64 + 0.5) * band;
            let c0 = if vis_groups.len() <= 1 {
                center - thickness / 2.0
            } else {
                center - inner / 2.0
                    + slot_index as f64 * (inner / vis_groups.len() as f64)
                    + (inner / vis_groups.len() as f64 - thickness) / 2.0
            };
            let c1 = c0 + thickness;
            let cm = (c0 + c1) / 2.0;
            let color = Style::rgba(ctx.series_color(s.group));
            let ink = Style::rgba(ctx.style.ink);
            let heading = if multi {
                format!("{} · {}", categories[s.category], groups[s.group])
            } else {
                categories[s.category].clone()
            };
            let index = observations.len();
            match kind {
                Kind2::Box => {
                    let Some(sum) = &s.summary else { continue };
                    let Some(text) = &s.box_texts else { continue };
                    let whisker = Stroke::solid(color, 1.5);
                    items.push(Item::Line {
                        path: Path::polyline(&[pt(cm, sum.lower_whisker), pt(cm, sum.q1)]),
                        stroke: whisker.clone(),
                        role: Role::Mark,
                        series: Some(s.group),
                        obs: Some(index),
                    });
                    items.push(Item::Line {
                        path: Path::polyline(&[pt(cm, sum.q3), pt(cm, sum.upper_whisker)]),
                        stroke: whisker.clone(),
                        role: Role::Mark,
                        series: Some(s.group),
                        obs: Some(index),
                    });
                    for w in [sum.lower_whisker, sum.upper_whisker] {
                        items.push(Item::Line {
                            path: Path::polyline(&[pt(cm - thickness * 0.25, w), pt(cm + thickness * 0.25, w)]),
                            stroke: whisker.clone(),
                            role: Role::Mark,
                            series: Some(s.group),
                            obs: Some(index),
                        });
                    }
                    let rect = span(c0, c1, sum.q1, sum.q3);
                    let hatch = ctx.series[s.group].hatch;
                    items.push(Item::Rect {
                        rect,
                        fill: Fill {
                            color: color.with_alpha(0.22),
                            hatch,
                            hatch_ink: hatch.map(|_| color),
                            hatch_scale: 1.0,
                        },
                        stroke: Some(Stroke::solid(color, 1.5)),
                        radius: [0.0; 4],
                        role: Role::Mark,
                        series: Some(s.group),
                        obs: Some(index),
                    });
                    items.push(Item::Line {
                        path: Path::polyline(&[pt(c0, sum.median), pt(c1, sum.median)]),
                        stroke: Stroke::solid(ink, 2.0),
                        role: Role::Mark,
                        series: Some(s.group),
                        obs: Some(index),
                    });
                    let f = &readout;
                    let lines = vec![
                        ReadoutLine {
                            name: "Observations".into(),
                            value: sum.count.to_string(),
                            series: None,
                            names_series: false,
                            derived: true,
                        },
                        ReadoutLine {
                            name: "Upper whisker".into(),
                            value: text.upper_whisker.clone(),
                            series: None,
                            names_series: false,
                            derived: true,
                        },
                        ReadoutLine {
                            name: "Third quartile".into(),
                            value: text.q3.clone(),
                            series: None,
                            names_series: false,
                            derived: true,
                        },
                        ReadoutLine {
                            name: "Median".into(),
                            value: text.median.clone(),
                            series: Some(s.group),
                            names_series: false,
                            derived: true,
                        },
                        ReadoutLine {
                            name: "First quartile".into(),
                            value: text.q1.clone(),
                            series: None,
                            names_series: false,
                            derived: true,
                        },
                        ReadoutLine {
                            name: "Lower whisker".into(),
                            value: text.lower_whisker.clone(),
                            series: None,
                            names_series: false,
                            derived: true,
                        },
                    ];
                    let area = span(c0, c1, sum.lower_whisker, sum.upper_whisker);
                    observations.push(Observation {
                        row: None,
                        dots: vec![],
                        series: Some(s.group),
                        at: pt(cm, sum.median),
                        wedge: None,
                        area: Some(area),
                        radius: 0.0,
                        x: cm,
                        readout: Readout {
                            heading: heading.clone(),
                            lines,
                        }
                        .into(),
                    });
                    for &o in &sum.outliers {
                        let v = s.samples.iter().find(|(r, _)| *r == o).map_or(0.0, |(_, v)| *v);
                        let at = pt(cm, v);
                        items.push(Item::Point {
                            center: at,
                            radius: 3.0,
                            shape: Shape::Circle,
                            fill: None,
                            stroke: Some(Stroke::solid(color, 1.5)),
                            role: Role::Mark,
                            series: Some(s.group),
                            obs: Some(observations.len()),
                        });
                        observations.push(Observation {
                            row: Some(o),
                            dots: vec![],
                            series: Some(s.group),
                            at,
                            wedge: None,
                            area: None,
                            radius: POINT_RADIUS,
                            x: at[0],
                            readout: Readout {
                                heading: format!("{heading} · row {}", o + 1),
                                lines: vec![ReadoutLine {
                                    name: title.clone(),
                                    value: numbers::value(v, f),
                                    series: Some(s.group),
                                    names_series: false,
                                    derived: false,
                                }],
                            }
                            .into(),
                        });
                    }
                }
                Kind2::Interval => {
                    let Some((a, e, b)) = s.interval else { continue };
                    let (va, vb) = (block.y.min.map_or(a, |m| a.max(m)), block.y.max.map_or(b, |m| b.min(m)));
                    let stroke = Stroke::solid(color, 2.0);
                    items.push(Item::Line {
                        path: Path::polyline(&[pt(cm, va), pt(cm, vb)]),
                        stroke: stroke.clone(),
                        role: Role::Mark,
                        series: Some(s.group),
                        obs: Some(index),
                    });
                    for (v, cut) in [(va, va != a), (vb, vb != b)] {
                        if !cut {
                            items.push(Item::Line {
                                path: Path::polyline(&[pt(cm - thickness * 0.3, v), pt(cm + thickness * 0.3, v)]),
                                stroke: stroke.clone(),
                                role: Role::Mark,
                                series: Some(s.group),
                                obs: Some(index),
                            });
                        }
                    }
                    if let Some(e) = e {
                        items.push(Item::Point {
                            center: pt(cm, e),
                            radius: 4.0,
                            shape: ctx.series[s.group].shape.unwrap_or(Shape::Circle),
                            fill: Some(Fill::solid(color)),
                            stroke: None,
                            role: Role::Mark,
                            series: Some(s.group),
                            obs: Some(index),
                        });
                    }
                    let mut lines = vec![
                        ReadoutLine {
                            name: "Upper".into(),
                            value: numbers::value(b, &readout),
                            series: None,
                            names_series: false,
                            derived: false,
                        },
                        ReadoutLine {
                            name: "Lower".into(),
                            value: numbers::value(a, &readout),
                            series: None,
                            names_series: false,
                            derived: false,
                        },
                    ];
                    if let Some(e) = e {
                        lines.insert(
                            1,
                            ReadoutLine {
                                name: block.y.label.clone().unwrap_or_else(|| y_field.clone()),
                                value: numbers::value(e, &readout),
                                series: Some(s.group),
                                names_series: false,
                                derived: false,
                            },
                        );
                    }
                    let area = span(cm - thickness / 2.0, cm + thickness / 2.0, va, vb).inflate(2.0);
                    observations.push(Observation {
                        row: Some(s.rows[0]),
                        dots: vec![],
                        series: Some(s.group),
                        at: pt(cm, e.unwrap_or(vb)),
                        wedge: None,
                        area: Some(area),
                        radius: 0.0,
                        x: cm,
                        readout: Readout {
                            heading: heading.clone(),
                            lines,
                        }
                        .into(),
                    });
                }
            }
        }
        let mut panel = Panel {
            facet: None,
            frame: *frame,
            plot,
            items,
            observations,
            targeting: Targeting::Contain,
            x: None,
            failure: None,
        };
        super::panel_title(ctx, &mut panel, name);
        panels.push(panel);
    }
    ctx.summary = match kind {
        Kind2::Box => {
            let medians: Vec<(&box_summary::Summary, &BoxTexts)> = slots
                .iter()
                .filter_map(|s| s.summary.as_ref().zip(s.box_texts.as_ref()))
                .collect();
            if medians.is_empty() {
                "No observations.".into()
            } else {
                let a = medians.iter().min_by(|a, b| a.0.median.total_cmp(&b.0.median)).unwrap();
                let b = medians.iter().max_by(|a, b| a.0.median.total_cmp(&b.0.median)).unwrap();
                format!(
                    "{}; medians from {} to {}.",
                    super::count(medians.len(), "group", "groups"),
                    a.1.median_label,
                    b.1.median_label
                )
            }
        }
        Kind2::Interval => {
            let shown: Vec<(f64, f64)> = slots
                .iter()
                .filter_map(|s| s.interval.map(|(a, _, b)| (a, b)))
                .collect();
            let (a, b) = (
                shown.iter().map(|s| s.0).fold(f64::INFINITY, f64::min),
                shown.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max),
            );
            if shown.is_empty() {
                "No intervals.".into()
            } else {
                format!(
                    "{} intervals spanning {} to {}.",
                    shown.len(),
                    numbers::value(a, &label_format),
                    numbers::value(b, &label_format)
                )
            }
        }
    };
    let legend = legend_for(
        ctx,
        if kind == Kind2::Box {
            Swatch::Square
        } else {
            Swatch::Line
        },
    );
    Painted {
        height: super::grid_height(facets.len(), columns, ph, super::FACET_GAP_Y),
        panels,
        figure_items: vec![],
        legend,
        failure: None,
    }
}

/// Bounds compare as decimal source text, so a lower bound above its upper bound is caught even
/// when both round to the same binary value (FEA-CH-70).
fn decimal_greater(row: &Value, lo: &str, hi: &str) -> bool {
    let text = |f: &str| match rows::cell(row, f) {
        Value::String(s) => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    };
    match (text(lo), text(hi)) {
        (Some(a), Some(b)) => crate::data::compare_decimal_text(&a, &b) == Some(std::cmp::Ordering::Greater),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::block::ChartBlock;
    use crate::geometry::{Item, Role};
    use crate::layout::{ChartTheme, Options, layout};
    use serde_json::json;

    fn opts() -> Options {
        Options {
            width: 640.0,
            ..Options::default()
        }
    }

    #[test]
    fn histograms_bin_with_sturges_and_disclose_exclusions() {
        let block =
            ChartBlock::from_value(&json!({"type": "histogram", "title": "T", "encoding": {"x": "v"}})).unwrap();
        let mut rows: Vec<_> = (0..100).map(|i| json!({"v": (i as f64 * 0.37).sin() * 10.0})).collect();
        rows.push(json!({"v": null}));
        let g = layout(&block, &rows, &ChartTheme::default(), &opts());
        assert!(g.failure.is_none());
        let bars = g.panels[0].observations.len();
        assert_eq!(bars, 8, "Sturges: ceil(log2 100) + 1");
        assert!(g.checks.iter().any(|c| c.kind == "excluded-observations"));
        assert!(
            g.note.as_deref().unwrap_or("").contains("100 observations"),
            "{:?}",
            g.note
        );
    }

    #[test]
    fn box_plots_summarise_with_quartiles_and_outliers() {
        let block = ChartBlock::from_value(&json!({"type": "boxplot", "title": "T", "encoding": {"x": "g", "y": "v"}}))
            .unwrap();
        let mut rows: Vec<_> = (1..=20).map(|i| json!({"g": "A", "v": i})).collect();
        rows.push(json!({"g": "A", "v": 100}));
        rows.extend((1..=10).map(|i| json!({"g": "B", "v": i * 2})));
        let g = layout(&block, &rows, &ChartTheme::default(), &opts());
        assert!(g.failure.is_none());
        let outliers = g.panels[0]
            .items
            .iter()
            .filter(|i| matches!(i, Item::Point { role: Role::Mark, .. }))
            .count();
        assert_eq!(outliers, 1);
        assert!(g.note.as_deref().unwrap().contains("box = middle half"));
    }

    #[test]
    fn intervals_reject_inverted_bounds() {
        let block = ChartBlock::from_value(
            &json!({"type": "interval", "title": "T", "encoding": {"x": "c", "lower": "lo", "upper": "hi"}}),
        )
        .unwrap();
        let rows = vec![json!({"c": "A", "lo": 1, "hi": 2}), json!({"c": "B", "lo": 3, "hi": 2})];
        let g = layout(&block, &rows, &ChartTheme::default(), &opts());
        assert!(g.failure.is_some());
        let ok = vec![json!({"c": "A", "lo": 1, "hi": 2}), json!({"c": "B", "lo": 2, "hi": 2})];
        let g = layout(&block, &ok, &ChartTheme::default(), &opts());
        assert!(g.failure.is_none(), "zero-width ranges are valid");
        assert_eq!(g.panels[0].observations.len(), 2);
    }
}
