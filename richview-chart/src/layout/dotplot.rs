//! Dot plots: the values of one numeric field in equal-width intervals, one dot for every
//! observation stacked up from the axis, groups stacked in series order. The count of a
//! stack is read on the vertical axis, so a dot is exactly one observation; its horizontal
//! position is the centre of its interval, which the note says (FEA-CH-42). All panels share
//! the same intervals.

use super::kit::{self, ValueSide, XAxis};
use super::sample::{disclose_excluded, read_number};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::axis::Zero;
use crate::binning;
use crate::diag::Level;
use crate::geometry::{Fill, Item, Observation, Readout, ReadoutLine, Rect, Role, Shape, Stroke, Swatch, Targeting};
use crate::numbers::{self, Format, Kind};
use crate::rows;

const RATIO: f64 = 16.0 / 9.0;
/// Intervals across the data.
const INTERVALS: usize = 30;
/// Largest and smallest dot radius, in pixels.
const MAX_RADIUS: f64 = 9.0;
const MIN_RADIUS: f64 = 1.5;

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let Some(x_field) = block.encoding.x.clone() else {
        let reason = "Cannot draw this dot plot · It needs encoding.x (the value)";
        ctx.check(
            "source-error",
            Level::Error,
            "The dot plot has no value",
            Some("Give encoding.x."),
            Some("chart.encoding".into()),
            None,
            serde_json::Value::Null,
        );
        return kit::failed(ctx, reason, RATIO);
    };
    let x_title = block.x.label.clone().unwrap_or_else(|| x_field.clone());
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![x_title.clone()],
    };
    let multi = block.encoding.color.is_some();
    ctx.set_series(&groups);
    let x_readout = block.readout_format(&block.x, ctx.locale);
    let count_format = Format {
        kind: Kind::Number,
        unit: None,
        digits: numbers::MAX_DIGITS,
        locale: ctx.locale,
    };
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    // The (group, value) of every observation, per panel.
    let mut per_panel: Vec<Vec<(usize, f64)>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut observed: Vec<(usize, f64)> = Vec::new();
        for &i in members {
            let row = &ctx.rows[i];
            let group = match &block.encoding.color {
                Some(f) => match rows::text(row, f).and_then(|g| groups.iter().position(|x| *x == g)) {
                    Some(g) => g,
                    None => {
                        excluded += 1;
                        continue;
                    }
                },
                None => 0,
            };
            match read_number(ctx, "dot plot", &mut failures[fi], i, &x_field) {
                Ok(Some(v)) => observed.push((group, v)),
                Ok(None) => excluded += 1,
                Err(()) => {}
            }
        }
        if observed.is_empty() && failures[fi].is_none() {
            failures[fi] = Some("Cannot draw this dot plot · No row has a value".into());
        }
        per_panel.push(observed);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "a group or value");
    let all: Vec<f64> = per_panel
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .flat_map(|(p, _)| p.iter().map(|o| o.1))
        .collect();
    // The edges and the counts jump with the data, so the exact rows decide them (TYP-36).
    let resolved: Result<Vec<f64>, String> = ctx.derive_projected(0, "dotplot.edges", || {
        let e = binning::equal_edges(&all, Some(INTERVALS), None).map_err(str::to_owned)?;
        binning::shown_edges(&e, &ctx.block.label_format(&ctx.block.x, ctx.locale)).map_err(str::to_owned)
    });
    let edges = match resolved {
        Ok(e) => e,
        Err(why) => {
            let reason = format!("Cannot draw this dot plot · {why}");
            ctx.check(
                "source-error",
                Level::Error,
                reason.clone(),
                Some("Give values that are finite numbers within a range RichView can represent."),
                Some("chart.encoding.x".into()),
                None,
                serde_json::Value::Null,
            );
            return kit::failed(ctx, &reason, RATIO);
        }
    };
    let bins = edges.len() - 1;
    // Observations per interval and group, for each panel.
    let counts: Vec<Vec<Vec<usize>>> = per_panel
        .iter()
        .zip(&failures)
        .enumerate()
        .map(|(fi, (observed, failure))| {
            ctx.derive(fi, "dotplot.counts", || {
                let mut c = vec![vec![0usize; groups.len()]; bins];
                if failure.is_none() {
                    for (g, v) in observed {
                        if let Some(k) = binning::bin_index(&edges, *v) {
                            c[k][*g] += 1;
                        }
                    }
                }
                c
            })
        })
        .collect();
    let visible: Vec<usize> = (0..groups.len()).filter(|g| !ctx.is_hidden(*g)).collect();
    let tallest = |fi: usize| -> usize {
        counts[fi]
            .iter()
            .map(|cell| visible.iter().map(|g| cell[*g]).sum::<usize>())
            .max()
            .unwrap_or(0)
    };
    let shared = (0..facets.len())
        .filter(|&fi| failures[fi].is_none())
        .map(&tallest)
        .max()
        .unwrap_or(0);
    let plan = kit::Plan::xy(ctx, facets.len(), block.style.ratio.unwrap_or(RATIO));
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let title = block.y.label.clone().unwrap_or_else(|| "Observations".into());
    let extent = [edges[0], edges[bins]];
    let surface = Style::rgba(ctx.style.surface);
    let mut panels = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let top = if block.style.free_scales { tallest(fi) } else { shared };
        let y = super::Fit::widen([0.0, top.max(1) as f64], ctx.wanted().y);
        ctx.fit.x = Some(extent);
        ctx.fit.y = Some(y);
        let side = ValueSide {
            data: y,
            zero: Zero::Include,
            title: title.clone(),
            format: &count_format,
            min: Some(0.0),
            max: None,
        };
        let frame = kit::xy_frame(ctx, &plan, outer, &XAxis::Numeric { extent }, Some(&x_title), &side);
        let unit = (frame.y(0.0) - frame.y(1.0)).abs();
        let pitch = (frame.x(edges[1]) - frame.x(edges[0])).abs();
        let radius = (0.46 * unit.min(pitch)).clamp(MIN_RADIUS, MAX_RADIUS);
        let mut marks: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        for (k, cell) in counts[fi].iter().enumerate() {
            let total: usize = visible.iter().map(|g| cell[*g]).sum();
            if total == 0 {
                continue;
            }
            let (left, right) = (frame.x(edges[k]), frame.x(edges[k + 1]));
            let centre = frame.x(edges[k] / 2.0 + edges[k + 1] / 2.0);
            if centre - radius < frame.plot.x - 0.5 || centre + radius > frame.plot.right() + 0.5 {
                continue;
            }
            let index = observations.len();
            let mut stacked = 0usize;
            let mut lines = Vec::new();
            let mut first: Option<usize> = None;
            for &g in &visible {
                if cell[g] == 0 {
                    continue;
                }
                first.get_or_insert(g);
                let color = Style::rgba(ctx.series_color(g));
                let shape = ctx.series[g].shape.unwrap_or(Shape::Circle);
                for _ in 0..cell[g] {
                    marks.push(Item::Point {
                        center: [centre, frame.y(stacked as f64 + 0.5)],
                        radius,
                        shape,
                        fill: Some(Fill::solid(color)),
                        stroke: Some(Stroke::solid(surface, 0.75)),
                        role: Role::Mark,
                        series: Some(g),
                        obs: Some(index),
                    });
                    stacked += 1;
                }
                lines.push(ReadoutLine {
                    label: None,
                    name: if multi {
                        groups[g].clone()
                    } else {
                        "Observations".into()
                    },
                    value: numbers::value(cell[g] as f64, &count_format),
                    series: Some(g),
                    names_series: multi,
                    derived: true,
                });
            }
            let column = Rect::new(
                left.min(right).max(frame.plot.x),
                frame.plot.y,
                (left.max(right).min(frame.plot.right()) - left.min(right).max(frame.plot.x)).max(1.0),
                frame.plot.h,
            );
            observations.push(Observation {
                row: None,
                dots: vec![],
                series: first,
                at: [centre, frame.y(total as f64 - 0.5)],
                wedge: None,
                area: Some(column),
                radius: 0.0,
                x: centre,
                readout: Readout {
                    heading: format!(
                        "{} to {}",
                        numbers::value(edges[k], &x_readout),
                        numbers::value(edges[k + 1], &x_readout)
                    ),
                    lines,
                }
                .into(),
            });
        }
        let mut panel = kit::xy_panel(ctx, outer, frame, marks, observations, name);
        panel.targeting = Targeting::Contain;
        panels.push(panel);
    }
    let shown: usize = counts
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .flat_map(|(c, _)| c.iter())
        .map(|cell| visible.iter().map(|g| cell[*g]).sum::<usize>())
        .sum();
    ctx.note.push(format!(
        "each dot is one observation, drawn at the centre of its interval of width {}; an interval includes its lower edge",
        numbers::value(edges[1] - edges[0], &x_readout)
    ));
    ctx.summary = format!(
        "{} in {}.",
        super::count(shown, "observation", "observations"),
        super::count(bins, "interval", "intervals")
    );
    let legend = kit::legend(ctx, Swatch::Dot);
    kit::finish(&plan, facets.len(), panels, legend)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_radius_stays_within_its_bounds() {
        for (unit, pitch) in [(0.0f64, 0.0f64), (1000.0, 1000.0), (4.0, 100.0)] {
            let r = (0.46 * unit.min(pitch)).clamp(MIN_RADIUS, MAX_RADIUS);
            assert!((MIN_RADIUS..=MAX_RADIUS).contains(&r));
        }
    }
}
