//! Two-dimensional bins: the plane cut into rectangles of equal width and height, each coloured by
//! the number of observations inside it and printing that number, with a key that decodes the
//! colour only when a cell cannot hold its count. The cells are computed by RichView, so every
//! count is derived and the note names the bins (FEA-CH-42).
//! Empty cells stay plain, so nothing is drawn where no observation is. All panels share the same
//! bins and the same colour scale.

use super::kit::{self, ValueSide, XAxis};
use super::sample::{COUNT_ROOM, Count, count_in_cell, decode_counts, disclose_excluded, read_number, shade};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::axis::Zero;
use crate::binning;
use crate::diag::Level;
use crate::geometry::{Fill, Item, Observation, Readout, ReadoutLine, Rect, Role, Stroke, Targeting};
use crate::numbers::{self, Format, Kind};

const RATIO: f64 = 16.0 / 9.0;
/// Fewest and most bins along an axis.
const MIN_BINS: usize = 5;
const MAX_BINS: usize = 24;

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(x_field), Some(y_field)) = (block.encoding.x.clone(), block.encoding.y.clone()) else {
        let reason = "Cannot draw this 2-D bin chart · It needs encoding.x and encoding.y";
        ctx.check(
            "source-error",
            Level::Error,
            "The 2-D bin chart has no x or no y",
            Some("Give encoding.x and encoding.y."),
            Some("chart.encoding".into()),
            None,
            serde_json::Value::Null,
        );
        return kit::failed(ctx, reason, RATIO);
    };
    let x_title = block.x.label.clone().unwrap_or_else(|| x_field.clone());
    let y_title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let x_readout = block.readout_format(&block.x, ctx.locale);
    let y_readout = block.readout_format(&block.y, ctx.locale);
    let y_labels = block.label_format(&block.y, ctx.locale);
    let count_format = Format {
        kind: Kind::Number,
        unit: None,
        digits: numbers::MAX_DIGITS,
        locale: ctx.locale,
    };
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut per_panel: Vec<Vec<[f64; 2]>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut points: Vec<[f64; 2]> = Vec::new();
        for &i in members {
            let x = read_number(ctx, "2-D bin chart", &mut failures[fi], i, &x_field);
            let y = read_number(ctx, "2-D bin chart", &mut failures[fi], i, &y_field);
            match (x, y) {
                (Ok(Some(x)), Ok(Some(y))) => points.push([x, y]),
                (Ok(_), Ok(_)) => excluded += 1,
                _ => {}
            }
        }
        if points.is_empty() && failures[fi].is_none() {
            failures[fi] = Some("Cannot draw this 2-D bin chart · No row has both an x and a y".into());
        }
        per_panel.push(points);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "an x or a y");
    let good: Vec<&Vec<[f64; 2]>> = per_panel
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .map(|(p, _)| p)
        .collect();
    let total: usize = good.iter().map(|p| p.len()).sum();
    let per_axis = ((total as f64).sqrt().ceil() as usize).clamp(MIN_BINS, MAX_BINS);
    let xs: Vec<f64> = good.iter().flat_map(|p| p.iter().map(|q| q[0])).collect();
    let ys: Vec<f64> = good.iter().flat_map(|p| p.iter().map(|q| q[1])).collect();
    // The edges and the counts jump with the data, so the exact rows decide them (TYP-36).
    let edges: Result<(Vec<f64>, Vec<f64>), String> = ctx.derive_projected(0, "bin2d.edges", || {
        let (x, y) = binning::equal_edges(&xs, Some(per_axis), None)
            .and_then(|x| binning::equal_edges(&ys, Some(per_axis), None).map(|y| (x, y)))
            .map_err(str::to_owned)?;
        let x = binning::shown_edges(&x, &ctx.block.label_format(&ctx.block.x, ctx.locale)).map_err(str::to_owned)?;
        let y = binning::shown_edges(&y, &ctx.block.label_format(&ctx.block.y, ctx.locale)).map_err(str::to_owned)?;
        Ok((x, y))
    });
    let (ex, ey) = match edges {
        Ok(e) => e,
        Err(why) => {
            let reason = format!("Cannot draw this 2-D bin chart · {why}");
            ctx.check(
                "source-error",
                Level::Error,
                reason.clone(),
                Some("Give finite numbers within a range RichView can represent."),
                Some("chart.encoding".into()),
                None,
                serde_json::Value::Null,
            );
            return kit::failed(ctx, &reason, RATIO);
        }
    };
    let (bx, by) = (ex.len() - 1, ey.len() - 1);
    // Observations in each cell, per panel: `cells[panel][column * by + line]`.
    let cells: Vec<Vec<usize>> = per_panel
        .iter()
        .zip(&failures)
        .enumerate()
        .map(|(fi, (points, failure))| {
            ctx.derive(fi, "bin2d.cells", || {
                let mut c = vec![0usize; bx * by];
                if failure.is_none() {
                    for p in points {
                        if let (Some(i), Some(j)) = (binning::bin_index(&ex, p[0]), binning::bin_index(&ey, p[1])) {
                            c[i * by + j] += 1;
                        }
                    }
                }
                c
            })
        })
        .collect();
    let top = cells.iter().flatten().copied().max().unwrap_or(0);
    let ramp = ctx.style.ramp;
    let plan = kit::Plan::xy(ctx, facets.len(), block.style.ratio.unwrap_or(RATIO));
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let surface = Style::rgba(ctx.style.surface);
    let x_extent = [ex[0], ex[bx]];
    let y_extent = [ey[0], ey[by]];
    let mut panels = Vec::new();
    let mut counts: Vec<Count> = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        ctx.fit.x = Some(x_extent);
        ctx.fit.y = Some(y_extent);
        let side = ValueSide {
            data: y_extent,
            zero: Zero::None,
            title: y_title.clone(),
            format: &y_labels,
            min: Some(y_extent[0]),
            max: Some(y_extent[1]),
        };
        let frame = kit::xy_frame(
            ctx,
            &plan,
            outer,
            &XAxis::Numeric { extent: x_extent },
            Some(&x_title),
            &side,
        );
        let mut marks: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        for i in 0..bx {
            let (left, right) = (
                frame.x(ex[i]).max(frame.plot.x),
                frame.x(ex[i + 1]).min(frame.plot.right()),
            );
            if right - left < 0.5 {
                continue;
            }
            for j in 0..by {
                let count = cells[fi][i * by + j];
                if count == 0 {
                    continue;
                }
                let (bottom, upper) = (frame.y(ey[j]), frame.y(ey[j + 1]));
                let rect = Rect::new(left, upper.min(bottom), right - left, (bottom - upper).abs());
                let index = observations.len();
                let fill = shade(&ramp, count, top);
                let text = numbers::value(count as f64, &count_format);
                counts.push(count_in_cell(ctx, [fi, index], &text, rect.center(), fill, |own| {
                    own.w + 2.0 * COUNT_ROOM <= rect.w && own.h + 2.0 * COUNT_ROOM <= rect.h
                }));
                marks.push(Item::Rect {
                    rect,
                    fill: Fill::solid(Style::rgba(fill)),
                    stroke: Some(Stroke::solid(surface, 0.5)),
                    radius: [0.0; 4],
                    role: Role::Mark,
                    series: None,
                    obs: Some(index),
                });
                let range =
                    |lo: f64, hi: f64, f: &Format| format!("{}–{}", numbers::value(lo, f), numbers::value(hi, f));
                observations.push(Observation {
                    row: None,
                    dots: vec![],
                    series: None,
                    at: rect.center(),
                    wedge: None,
                    area: Some(rect),
                    radius: 0.0,
                    x: rect.center()[0],
                    readout: Readout {
                        heading: format!(
                            "{} × {}",
                            range(ex[i], ex[i + 1], &x_readout),
                            range(ey[j], ey[j + 1], &y_readout)
                        ),
                        lines: vec![ReadoutLine {
                            label: None,
                            name: "Observations".into(),
                            value: text,
                            series: None,
                            names_series: false,
                            derived: true,
                        }],
                    }
                    .into(),
                });
            }
        }
        let mut panel = kit::xy_panel(ctx, outer, frame, marks, observations, name);
        panel.targeting = Targeting::Contain;
        panels.push(panel);
    }
    ctx.note.push(format!(
        "cells are {bx} by {by} equal-width bins; a bin includes its lower edge, and empty bins are left plain"
    ));
    ctx.summary = format!(
        "{} in {bx} by {by} bins, coloured by count up to {top}.",
        super::count(total, "observation", "observations")
    );
    let mut painted = kit::finish(&plan, facets.len(), panels, None);
    decode_counts(ctx, &mut painted, top, &count_format, counts);
    painted
}
