//! Vector fields: an arrow at each position `x`, `y` pointing in the direction `angle`, in degrees
//! counter-clockwise from east on the screen. With a value in `encoding.size`, an arrow is as long
//! as its value is large against the largest and is coloured along the style's ramp, with a key
//! that decodes the colour; without one every arrow has the same length. An arrow is centred on
//! its position and all arrows stay inside the plot, which the axes make room for. Nothing is
//! computed, so every readout is the author's own (FEA-CH-42).

use super::contour::{add_value_key, line_color};
use super::kit::{self, ValueSide, XAxis};
use super::sample::{disclose_excluded, read_number};
use super::types::Facets;
use super::{Ctx, LABEL_GAP, Painted, Style};
use crate::axis::Zero;
use crate::derived::{DataRange, Position};
use crate::diag::Level;
use crate::geometry::{Cap, Fill, Item, Observation, Path, Readout, ReadoutLine, Role, Shape, Stroke, Targeting};
use crate::limits;
use crate::numbers::{self, Format, Kind};
use serde::{Deserialize, Serialize};

const RATIO: f64 = 16.0 / 9.0;
/// Longest and shortest arrow, in pixels, whatever the spacing of the positions.
const LONGEST: f64 = 40.0;
const SHORTEST: f64 = 10.0;
/// Length when there is one position or the positions coincide.
const DEFAULT_LENGTH: f64 = 28.0;
const WIDTH: f64 = 1.6;
/// An arrow shorter than this is a dot.
const DOT_BELOW: f64 = 3.0;

struct Arrow {
    row: usize,
    x: f64,
    y: f64,
    angle: f64,
    value: Option<f64>,
}

#[derive(Serialize, Deserialize)]
struct Field {
    x: DataRange,
    y: DataRange,
    sample: Vec<[Position; 2]>,
}

/// The extent of `e` made wider by `pad_px` of a plot `length_px` long, so what is drawn within
/// `pad_px` of a position still fits; a single value gets a unit of room.
fn padded(e: [f64; 2], pad_px: f64, length_px: f64) -> [f64; 2] {
    let span = e[1] - e[0];
    let room = if span > 0.0 {
        span * pad_px / (length_px - 2.0 * pad_px).max(1.0)
    } else if e[0] == 0.0 {
        1.0
    } else {
        e[0].abs() * 0.1
    };
    [e[0] - room, e[1] + room]
}

/// The median distance from a position to its nearest other position, in pixels, for positions
/// laid out over a plot of `w` by `h` pixels; `None` when no two positions differ.
fn nearest_spacing(points: &[[f64; 2]], extent_x: [f64; 2], extent_y: [f64; 2], w: f64, h: f64) -> Option<f64> {
    let (sx, sy) = (extent_x[1] - extent_x[0], extent_y[1] - extent_y[0]);
    let map = |p: &[f64; 2]| {
        [
            if sx > 0.0 { (p[0] - extent_x[0]) / sx * w } else { 0.0 },
            if sy > 0.0 { (p[1] - extent_y[0]) / sy * h } else { 0.0 },
        ]
    };
    let px: Vec<[f64; 2]> = points.iter().map(map).collect();
    let mut nearest: Vec<f64> = Vec::new();
    for (i, a) in px.iter().enumerate() {
        let least = px
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, b)| (a[0] - b[0]).hypot(a[1] - b[1]))
            .filter(|d| *d > 0.0)
            .fold(f64::INFINITY, f64::min);
        if least.is_finite() {
            nearest.push(least);
        }
    }
    nearest.sort_by(f64::total_cmp);
    nearest.get(nearest.len() / 2).copied()
}

/// The pieces of an arrow of `length` pixels centred on `pivot` pointing `angle` degrees
/// counter-clockwise from east: the shaft's end points and the head's three corners, or only the
/// centre when the arrow is a dot.
struct Glyph {
    tail: [f64; 2],
    base: [f64; 2],
    head: Option<[[f64; 2]; 3]>,
}

fn glyph(pivot: [f64; 2], length: f64, angle: f64) -> Glyph {
    let a = angle.to_radians();
    let u = [a.cos(), -a.sin()];
    let half = length / 2.0;
    let tail = [pivot[0] - u[0] * half, pivot[1] - u[1] * half];
    let tip = [pivot[0] + u[0] * half, pivot[1] + u[1] * half];
    if length < DOT_BELOW {
        return Glyph {
            tail: pivot,
            base: pivot,
            head: None,
        };
    }
    let size = (0.35 * length).clamp(4.0, 9.0).min(length);
    let base = [tip[0] - u[0] * size, tip[1] - u[1] * size];
    let n = [-u[1], u[0]];
    let wing = size * 0.42;
    Glyph {
        tail,
        base,
        head: Some([
            tip,
            [base[0] + n[0] * wing, base[1] + n[1] * wing],
            [base[0] - n[0] * wing, base[1] - n[1] * wing],
        ]),
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(x_field), Some(y_field), Some(angle_field)) = (
        block.encoding.x.clone(),
        block.encoding.y.clone(),
        block.channel("angle"),
    ) else {
        let reason = "Cannot draw this vector chart · It needs encoding.x, encoding.y and encoding.angle";
        ctx.check(
            "source-error",
            Level::Error,
            "The vector chart has no x, y or angle",
            Some("Give encoding.x, encoding.y and encoding.angle."),
            Some("chart.encoding".into()),
            None,
            serde_json::Value::Null,
        );
        return kit::failed(ctx, reason, RATIO);
    };
    let value_field = block.encoding.size.clone();
    let x_title = block.x.label.clone().unwrap_or_else(|| x_field.clone());
    let y_title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let value_title = match &value_field {
        Some(f) => block.color.label.clone().unwrap_or_else(|| f.clone()),
        None => String::new(),
    };
    let x_readout = block.readout_format(&block.x, ctx.locale);
    let y_readout = block.readout_format(&block.y, ctx.locale);
    let value_readout = block.readout_format(&block.color, ctx.locale);
    let value_labels = block.label_format(&block.color, ctx.locale);
    let y_labels = block.label_format(&block.y, ctx.locale);
    let angle_format = Format {
        kind: Kind::Number,
        unit: Some("°".into()),
        digits: block.readout_digits,
        locale: ctx.locale,
    };
    ctx.set_series(&["Arrows".to_owned()]);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut per_panel: Vec<Vec<Arrow>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut arrows: Vec<Arrow> = Vec::new();
        for &i in members {
            let x = read_number(ctx, "vector chart", &mut failures[fi], i, &x_field);
            let y = read_number(ctx, "vector chart", &mut failures[fi], i, &y_field);
            let angle = read_number(ctx, "vector chart", &mut failures[fi], i, &angle_field);
            let value = match &value_field {
                Some(f) => read_number(ctx, "vector chart", &mut failures[fi], i, f),
                None => Ok(Some(0.0)),
            };
            match (x, y, angle, value) {
                (Ok(Some(x)), Ok(Some(y)), Ok(Some(angle)), Ok(Some(v))) => {
                    arrows.push(Arrow {
                        row: i,
                        x,
                        y,
                        angle,
                        value: value_field.as_ref().map(|_| v),
                    });
                }
                (Ok(_), Ok(_), Ok(_), Ok(_)) => excluded += 1,
                _ => {}
            }
        }
        // A value below zero leaves its arrow out; whether a value is below zero jumps with the data,
        // so the exact rows decide which rows stay (TYP-36).
        let kept: Vec<usize> = ctx.derive(fi, "vector.rows", || {
            let mut rows: Vec<usize> = arrows
                .iter()
                .filter(|a| a.value.is_none_or(|v| v >= 0.0))
                .map(|a| a.row)
                .collect();
            rows.sort_unstable();
            rows
        });
        let before = arrows.len();
        arrows.retain(|a| kept.binary_search(&a.row).is_ok());
        excluded += before - arrows.len();
        if arrows.len() > limits::max("vector", "arrows per panel") {
            failures[fi] = Some(kit::over_limit(
                ctx,
                "vector",
                "arrows per panel",
                arrows.len(),
                Some("chart.encoding.x"),
            ));
        }
        if arrows.is_empty() && failures[fi].is_none() {
            failures[fi] = Some(
                "Cannot draw this vector chart · No row has an x, a y, an angle and a value of zero or more".into(),
            );
        }
        per_panel.push(arrows);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "an x, a y, an angle or a value of zero or more");
    let live: Vec<&Vec<Arrow>> = per_panel
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .map(|(a, _)| a)
        .collect();
    let extent_of = |pick: fn(&Arrow) -> f64| {
        live.iter()
            .flat_map(|a| a.iter().map(pick))
            .fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], v| [lo.min(v), hi.max(v)])
    };
    let all: Vec<[f64; 2]> = live.iter().flat_map(|a| a.iter().map(|a| [a.x, a.y])).collect();
    // Spacing is judged within a panel's positions, which a thousand-point crowd keeps cheap.
    let sample: Vec<[f64; 2]> = all.iter().step_by(all.len().div_ceil(1_500).max(1)).copied().collect();
    // Exact rows choose membership; positions and spacing use the same source grids in both painters.
    let field: Field = ctx.derive_projected(0, "vector.field", || {
        let x = DataRange::encode(extent_of(|a| a.x), ctx.grid_for(&x_field), &x_readout);
        let y = DataRange::encode(extent_of(|a| a.y), ctx.grid_for(&y_field), &y_readout);
        let sample = sample
            .into_iter()
            .map(|[a, b]| {
                [
                    Position::encode(a, x.grid.as_ref(), &x_readout),
                    Position::encode(b, y.grid.as_ref(), &y_readout),
                ]
            })
            .collect();
        Field { x, y, sample }
    });
    let (xe, ye) = (field.x.bounds(), field.y.bounds());
    let sample: Vec<_> = field
        .sample
        .iter()
        .map(|p| [p[0].at(field.x.grid.as_ref()), p[1].at(field.y.grid.as_ref())])
        .collect();
    let (vmin, vmax) = live
        .iter()
        .flat_map(|a| a.iter().filter_map(|a| a.value))
        .fold((f64::INFINITY, 0.0f64), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let ratio = block.style.ratio.unwrap_or(RATIO);
    let plan = kit::Plan::xy(ctx, facets.len(), ratio);
    let frames = plan.frames(facets.len(), ctx.opts.width);
    // The plot's size as the axes will make it, to size the arrows and the room they need.
    let lh = ctx.line_height();
    let outer = frames[0];
    let plot_h = (outer.bottom() - (outer.y + plan.title_room + lh + 4.0 + lh / 2.0) - (lh + LABEL_GAP + lh)).max(60.0);
    let plot_w = (outer.w - 70.0).max(60.0);
    let spacing = nearest_spacing(&sample, xe, ye, plot_w, plot_h);
    let longest = spacing
        .map_or(DEFAULT_LENGTH, |d| 0.85 * d)
        .clamp(SHORTEST, LONGEST)
        .min(plot_h / 3.0)
        .max(4.0);
    let pad = longest / 2.0 + 6.0;
    let (x_extent, y_extent) = (padded(xe, pad, plot_w), padded(ye, pad, plot_h));
    let finite = |e: [f64; 2]| e[0].is_finite() && e[1].is_finite() && (e[1] - e[0]).is_finite() && e[1] > e[0];
    if !finite(x_extent) || !finite(y_extent) {
        let reason = "Cannot draw this vector chart · The values are too extreme to represent";
        ctx.check(
            "source-error",
            Level::Error,
            reason,
            Some("Give finite numbers within a range RichView can represent."),
            Some("chart.encoding".into()),
            None,
            serde_json::Value::Null,
        );
        return kit::failed(ctx, reason, RATIO);
    }
    let plain = Style::rgba(ctx.series_color(0));
    let mut panels = Vec::new();
    let mut drawn = 0usize;
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
        for a in &per_panel[fi] {
            let mut pivot = [
                frame.x(field.x.project(a.x, &x_readout)),
                frame.y(field.y.project(a.y, &y_readout)),
            ];
            // Left out of a zoomed window; the axes leave room for every arrow otherwise.
            if !frame.plot.contains(pivot) {
                continue;
            }
            let length = match a.value {
                Some(v) if vmax > 0.0 => longest * v / vmax,
                Some(_) => 0.0,
                None => longest,
            };
            let reach = length / 2.0 + 5.0;
            pivot[0] = pivot[0].clamp(
                frame.plot.x + reach,
                (frame.plot.right() - reach).max(frame.plot.x + reach),
            );
            pivot[1] = pivot[1].clamp(
                frame.plot.y + reach,
                (frame.plot.bottom() - reach).max(frame.plot.y + reach),
            );
            let color = match a.value {
                Some(v) => {
                    let t = if vmax > vmin { (v - vmin) / (vmax - vmin) } else { 0.5 };
                    Style::rgba(line_color(ctx, t))
                }
                None => plain,
            };
            let g = glyph(pivot, length, a.angle);
            let index = observations.len();
            if g.head.is_none() {
                marks.push(Item::Point {
                    center: pivot,
                    radius: 1.6,
                    shape: Shape::Circle,
                    fill: Some(Fill::solid(color)),
                    stroke: None,
                    role: Role::Mark,
                    series: Some(0),
                    obs: Some(index),
                });
            } else {
                marks.push(Item::Line {
                    path: Path::polyline(&[g.tail, g.base]),
                    stroke: Stroke {
                        cap: Cap::Round,
                        ..Stroke::solid(color, WIDTH)
                    },
                    role: Role::Mark,
                    series: Some(0),
                    obs: None,
                });
                if let Some(corners) = g.head {
                    let mut head = Path::polyline(&corners);
                    head.close();
                    marks.push(Item::Area {
                        path: head,
                        fill: Fill::solid(color),
                        role: Role::Mark,
                        series: Some(0),
                        obs: Some(index),
                    });
                }
            }
            let mut lines = Vec::new();
            if let Some(v) = a.value {
                lines.push(ReadoutLine {
                    label: None,
                    name: block
                        .color
                        .label
                        .clone()
                        .unwrap_or_else(|| super::cartesian::readable_name(value_field.as_deref().unwrap_or_default())),
                    value: numbers::value(v, &value_readout),
                    series: None,
                    names_series: false,
                    derived: false,
                });
            }
            lines.push(ReadoutLine {
                label: None,
                name: "Direction".into(),
                value: numbers::value(a.angle, &angle_format),
                series: None,
                names_series: false,
                derived: false,
            });
            let heading = format!(
                "Near {}, {}",
                numbers::readout_coordinate(a.x, &x_readout),
                numbers::readout_coordinate(a.y, &y_readout),
            );
            observations.push(Observation {
                row: Some(a.row),
                dots: vec![],
                series: Some(0),
                at: pivot,
                wedge: None,
                area: None,
                radius: (length / 2.0).max(6.0),
                x: pivot[0],
                readout: Readout { heading, lines }.into(),
            });
            drawn += 1;
        }
        let mut panel = kit::xy_panel(ctx, outer, frame, marks, observations, name);
        panel.targeting = Targeting::Radius;
        panels.push(panel);
    }
    ctx.note
        .push("direction is the angle in degrees counter-clockwise from east".into());
    if value_field.is_some() {
        ctx.note.push(format!(
            "arrow length and colour follow {value_title}; the longest arrow is the largest value"
        ));
    }
    ctx.summary = format!(
        "Vector field of {}{}.",
        super::count(drawn, "arrow", "arrows"),
        if value_field.is_some() {
            format!(", sized and coloured by {value_title}")
        } else {
            String::new()
        }
    );
    let mut painted = kit::finish(&plan, facets.len(), panels, None);
    if value_field.is_some() && vmax > 0.0 {
        let colors = |t: f64| line_color(ctx, t);
        add_value_key(
            ctx,
            &mut painted,
            &value_title,
            [vmin, vmax],
            &[],
            &value_labels,
            &colors,
        );
    }
    painted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_arrow_points_the_way_its_angle_says() {
        // East, then north (up the screen), then west.
        let east = glyph([10.0, 10.0], 20.0, 0.0);
        assert!(east.head.unwrap()[0][0] > east.tail[0]);
        let north = glyph([10.0, 10.0], 20.0, 90.0);
        assert!(north.head.unwrap()[0][1] < north.tail[1]);
        let west = glyph([10.0, 10.0], 20.0, 180.0);
        assert!(west.head.unwrap()[0][0] < west.tail[0]);
    }

    #[test]
    fn an_arrow_is_centred_on_its_position_and_a_tiny_one_is_a_dot() {
        let g = glyph([50.0, 40.0], 24.0, 37.0);
        let tip = g.head.unwrap()[0];
        let mid = [(g.tail[0] + tip[0]) / 2.0, (g.tail[1] + tip[1]) / 2.0];
        assert!((mid[0] - 50.0).abs() < 1e-9 && (mid[1] - 40.0).abs() < 1e-9);
        assert!(glyph([5.0, 5.0], 1.0, 10.0).head.is_none());
    }

    #[test]
    fn spacing_is_the_median_nearest_distance() {
        let grid: Vec<[f64; 2]> = (0..5).flat_map(|i| (0..5).map(move |j| [i as f64, j as f64])).collect();
        let d = nearest_spacing(&grid, [0.0, 4.0], [0.0, 4.0], 100.0, 100.0).unwrap();
        assert!((d - 25.0).abs() < 1e-9);
        assert!(nearest_spacing(&[[1.0, 1.0], [1.0, 1.0]], [1.0, 1.0], [1.0, 1.0], 100.0, 100.0).is_none());
    }

    #[test]
    fn room_for_the_arrows_widens_the_extent() {
        let e = padded([0.0, 10.0], 20.0, 300.0);
        assert!(e[0] < 0.0 && e[1] > 10.0);
        // 20 px each side of 260 px left over for 10 units.
        assert!((e[1] - 10.0 - 10.0 * 20.0 / 260.0).abs() < 1e-9);
        assert_eq!(padded([5.0, 5.0], 20.0, 300.0), [4.5, 5.5]);
        assert_eq!(padded([0.0, 0.0], 20.0, 300.0), [-1.0, 1.0]);
    }
}
