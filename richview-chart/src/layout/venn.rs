//! Venn diagrams of two or three sets. A row names the sets it counts in `encoding.sets`, joined
//! by `&` or a comma, and gives a size in `encoding.y`: a row with one set is that set's total,
//! a row with two or three is the size of the overlap of those sets (including whatever else lies
//! in it). Each circle has an area proportional to its set's total, and the distance between two
//! circles is the one at which their lens has the area of their overlap, so sets and pairs are
//! exact for two sets and for three when the three distances can meet (otherwise a warning says
//! so); the middle of three circles is wherever the geometry puts it. A reader points at a
//! circle or at an overlap and the outline decides which one (FEA-CH-42).

use super::graph;
use super::kit;
use super::sample::read_number;
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::derived::{Position, normalized_grid, statistic_grid};
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Stroke, Swatch,
    Targeting,
};
use crate::numbers::{self, Format, Kind};
use crate::payload::Quant;
use crate::rows;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::f64::consts::PI;

const RATIO: f64 = 4.0 / 3.0;
/// Sides of the polygons that stand for circles when overlaps are cut out of them.
const SIDES: usize = 120;
/// Gap, as a share of the radii, left between sets that share nothing.
const GAP: f64 = 0.04;
const SET_ALPHA: f64 = 0.42;
const LENS_ALPHA: f64 = 0.16;
const OUTLINE: f64 = 1.5;

/// The names in a `sets` cell: a list as JSON, or text joined by `&` or commas. Names are trimmed
/// and kept once each, in order.
fn parse_sets(text: &str) -> Vec<String> {
    let raw: Vec<String> = if text.trim_start().starts_with('[')
        && let Ok(list) = serde_json::from_str::<Vec<String>>(text)
    {
        list
    } else {
        text.split(['&', ',']).map(str::to_owned).collect()
    };
    let mut names: Vec<String> = Vec::new();
    for name in raw.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        if !names.iter().any(|n| n == name) {
            names.push(name.to_owned());
        }
    }
    names
}

/// The area two circles of radii `r1` and `r2` share when their centres are `d` apart.
fn lens_area(d: f64, r1: f64, r2: f64) -> f64 {
    if d >= r1 + r2 {
        return 0.0;
    }
    if d <= (r1 - r2).abs() {
        return PI * r1.min(r2).powi(2);
    }
    let a = r1 * r1 * ((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1)).clamp(-1.0, 1.0).acos();
    let b = r2 * r2 * ((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2)).clamp(-1.0, 1.0).acos();
    let k = ((-d + r1 + r2) * (d + r1 - r2) * (d - r1 + r2) * (d + r1 + r2))
        .max(0.0)
        .sqrt();
    a + b - 0.5 * k
}

/// The distance between centres at which circles of radii `r1` and `r2` share `area`. The sums
/// are done in units of the larger radius, so a very large or very small chart keeps its precision.
fn distance_for(r1: f64, r2: f64, area: f64) -> f64 {
    let unit = r1.max(r2);
    let (a, b, wanted) = (r1 / unit, r2 / unit, area / (unit * unit));
    let (near, far) = ((a - b).abs(), a + b);
    let d = if wanted <= 0.0 {
        far * (1.0 + GAP)
    } else if wanted >= PI * a.min(b).powi(2) * (1.0 - 1e-12) {
        near
    } else {
        let (mut lo, mut hi) = (near, far);
        for _ in 0..80 {
            let mid = (lo + hi) / 2.0;
            if lens_area(mid, a, b) > wanted {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (lo + hi) / 2.0
    };
    d * unit
}

/// A regular polygon standing for the circle at `c` with radius `r`, counter-clockwise.
fn circle_polygon(c: [f64; 2], r: f64) -> Vec<[f64; 2]> {
    (0..SIDES)
        .map(|k| {
            let a = TAU_OVER_SIDES * k as f64;
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

const TAU_OVER_SIDES: f64 = std::f64::consts::TAU / SIDES as f64;

/// The part of the convex polygon `subject` inside the convex polygon `clipper`, both counter-clockwise
/// (Sutherland-Hodgman).
fn clip_convex(subject: &[[f64; 2]], clipper: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = subject.to_vec();
    for k in 0..clipper.len() {
        if out.is_empty() {
            break;
        }
        let (a, b) = (clipper[k], clipper[(k + 1) % clipper.len()]);
        let side = |p: [f64; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        let input = std::mem::take(&mut out);
        for i in 0..input.len() {
            let (p, q) = (input[i], input[(i + 1) % input.len()]);
            let (sp, sq) = (side(p), side(q));
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                let t = sp / (sp - sq);
                out.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
            }
        }
    }
    out
}

/// A circle as a path of four Bézier arcs, whose control points stay inside its bounding square.
fn circle_path(c: [f64; 2], r: f64) -> Path {
    const K: f64 = 0.552_284_749_830_793_4;
    let mut path = Path::new();
    path.move_to([c[0] + r, c[1]]);
    path.curve_to([c[0] + r, c[1] + K * r], [c[0] + K * r, c[1] + r], [c[0], c[1] + r]);
    path.curve_to([c[0] - K * r, c[1] + r], [c[0] - r, c[1] + K * r], [c[0] - r, c[1]]);
    path.curve_to([c[0] - r, c[1] - K * r], [c[0] - K * r, c[1] - r], [c[0], c[1] - r]);
    path.curve_to([c[0] + K * r, c[1] - r], [c[0] + r, c[1] - K * r], [c[0] + r, c[1]]);
    path.close();
    path
}

/// A set: its name, its total and the row that gave it.
struct SetDef {
    name: String,
    total: f64,
    row: usize,
}

/// An overlap of two or three sets (indices into the sets, ascending) and its size.
struct Overlap {
    members: Vec<usize>,
    value: f64,
    row: usize,
}

/// One panel's sets and overlaps, or the reason it cannot be drawn.
struct Diagram {
    sets: Vec<SetDef>,
    overlaps: Vec<Overlap>,
    /// Centres in units where the largest set across all facets has radius one.
    centres: Vec<[f64; 2]>,
    /// Radii decoded from the shared normalized area grid.
    radii: Vec<f64>,
    /// Overlap rows left out for naming a set with no row of its own, or for repeating one.
    skipped: usize,
    /// Whether every pair sits at the distance its overlap asks for; three distances that break the
    /// triangle inequality cannot all be met.
    exact: bool,
}

impl Diagram {
    fn radius(&self, k: usize) -> f64 {
        self.radii[k]
    }
    /// The size of the overlap of `members`, zero when none is given.
    fn overlap(&self, members: &[usize]) -> f64 {
        self.overlaps
            .iter()
            .find(|o| o.members == members)
            .map_or(0.0, |o| o.value)
    }
    /// The bounding box of the circles.
    fn extent(&self) -> Rect {
        let mut e: Option<Rect> = None;
        for (k, c) in self.centres.iter().enumerate() {
            let r = self.radius(k);
            let b = Rect::new(c[0] - r, c[1] - r, 2.0 * r, 2.0 * r);
            e = Some(e.map_or(b, |a| a.union(&b)));
        }
        e.unwrap_or_default()
    }
}

/// Place the circles: the first at the origin, the second to its right at the distance its
/// overlap asks for, the third where its two distances meet (below, or on the line when the
/// distances cannot meet), with whether the distances all hold.
fn place(sets: &[SetDef], overlaps: &[Overlap]) -> (Vec<[f64; 2]>, bool) {
    let r: Vec<f64> = sets.iter().map(|s| s.total.sqrt()).collect();
    let pair = |i: usize, j: usize| {
        let value = overlaps.iter().find(|o| o.members == [i, j]).map_or(0.0, |o| o.value);
        distance_for(r[i], r[j], PI * value)
    };
    let mut centres = vec![[0.0, 0.0]];
    let dab = pair(0, 1);
    centres.push([dab, 0.0]);
    let mut exact = true;
    if sets.len() == 3 {
        let (dac, dbc) = (pair(0, 2), pair(1, 2));
        let tolerance = 1e-9 * (dab + dac + dbc).max(1.0);
        let (x, y) = if dab < 1e-9 {
            exact = (dac - dbc).abs() <= tolerance;
            (dac, 0.0)
        } else {
            let x = (dac * dac - dbc * dbc + dab * dab) / (2.0 * dab);
            let y2 = dac * dac - x * x;
            exact = y2 >= -tolerance * (dac + x.abs()).max(1.0);
            (x, y2.max(0.0).sqrt())
        };
        centres.push([x, y]);
    }
    (centres, exact)
}

/// What the sizes decide about a diagram, from the exact values: why it cannot be drawn, or where
/// the circles stand (TYP-36). An overlap as large as its set is an error on one side of a grid
/// step and fine on the other, and the placement is a solved geometry.
#[derive(Serialize, Deserialize)]
struct Solved<P = f64> {
    error: Option<String>,
    centres: Vec<[P; 2]>,
    areas: Vec<P>,
    exact: bool,
}

#[derive(Serialize, Deserialize)]
struct Solutions {
    area_grid: Quant,
    position_grid: Option<Quant>,
    panels: Vec<Option<Solved<Position>>>,
}

/// Why `diagram` cannot be drawn: an overlap larger than a set or than a pair of its sets.
fn invalid(diagram: &Diagram, what: &str) -> Option<String> {
    for o in &diagram.overlaps {
        let label = o
            .members
            .iter()
            .map(|&k| diagram.sets[k].name.as_str())
            .collect::<Vec<_>>()
            .join(" and ");
        for &k in &o.members {
            if o.value > diagram.sets[k].total {
                return Some(format!(
                    "Cannot draw this {what} · The overlap of {label} is larger than {}",
                    diagram.sets[k].name
                ));
            }
        }
        if o.members.len() == 3 {
            for skip in 0..3 {
                let sub: Vec<usize> = o
                    .members
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != skip)
                    .map(|(_, &k)| k)
                    .collect();
                if o.value > diagram.overlap(&sub) {
                    return Some(format!(
                        "Cannot draw this {what} · The overlap of {label} is larger than a pair of its sets"
                    ));
                }
            }
        }
    }
    None
}

/// Read the sets and overlaps of one facet's `entries` (names, size, row).
fn build(entries: &[(Vec<String>, f64, usize)], what: &str) -> Result<Diagram, String> {
    let mut sets: Vec<SetDef> = Vec::new();
    for (names, value, row) in entries.iter().filter(|e| e.0.len() == 1) {
        if sets.iter().any(|s| s.name == names[0]) {
            continue;
        }
        sets.push(SetDef {
            name: names[0].clone(),
            total: *value,
            row: *row,
        });
    }
    if sets.len() < 2 {
        return Err(format!(
            "Cannot draw this {what} · It needs a row with one set for each of two or three sets"
        ));
    }
    if sets.len() > 3 {
        return Err(format!(
            "Cannot draw this {what} · It draws two or three sets, and this has {}",
            sets.len()
        ));
    }
    let mut overlaps: Vec<Overlap> = Vec::new();
    let mut skipped = 0usize;
    for (names, value, row) in entries.iter().filter(|e| e.0.len() > 1) {
        let mut members: Vec<usize> = names
            .iter()
            .filter_map(|n| sets.iter().position(|s| s.name == *n))
            .collect();
        members.sort_unstable();
        members.dedup();
        if members.len() != names.len() || overlaps.iter().any(|o| o.members == members) {
            skipped += 1;
            continue;
        }
        overlaps.push(Overlap {
            members,
            value: *value,
            row: *row,
        });
    }
    Ok(Diagram {
        sets,
        overlaps,
        centres: Vec::new(),
        radii: Vec::new(),
        skipped,
        exact: true,
    })
}

/// The point of the circle `(c, r)` farthest from its edge and from the circles of `others`, and
/// that distance.
fn pole(c: [f64; 2], r: f64, others: &[([f64; 2], f64)]) -> ([f64; 2], f64) {
    const N: usize = 24;
    let mut best = (c, f64::NEG_INFINITY);
    for i in 0..=N {
        for j in 0..=N {
            let p = [
                c[0] - r + 2.0 * r * i as f64 / N as f64,
                c[1] - r + 2.0 * r * j as f64 / N as f64,
            ];
            let inside = r - (p[0] - c[0]).hypot(p[1] - c[1]);
            if inside <= 0.0 {
                continue;
            }
            let room = others
                .iter()
                .map(|(o, ro)| (p[0] - o[0]).hypot(p[1] - o[1]) - ro)
                .fold(inside, f64::min);
            if room > best.1 {
                best = (p, room);
            }
        }
    }
    best
}

/// Whether the box `b` lies inside the circle `own` and clear of the `others`.
fn fits(b: &Rect, own: ([f64; 2], f64), others: &[([f64; 2], f64)]) -> bool {
    for i in 0..=2 {
        for j in 0..=2 {
            let p = [b.x + b.w * i as f64 / 2.0, b.y + b.h * j as f64 / 2.0];
            if (p[0] - own.0[0]).hypot(p[1] - own.0[1]) > own.1 - 1.5 {
                return false;
            }
            if others.iter().any(|(o, ro)| (p[0] - o[0]).hypot(p[1] - o[1]) < ro + 1.5) {
                return false;
            }
        }
    }
    true
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(sets_field), Some(value_field)) = (block.channel("sets"), block.encoding.y.clone()) else {
        let reason = "Cannot draw this Venn diagram · It needs encoding.sets and encoding.y";
        ctx.check(
            "source-error",
            Level::Error,
            "The Venn diagram has no sets or no y",
            Some("Give encoding.sets for the sets of each row and encoding.y for its size."),
            Some("chart.encoding".into()),
            None,
            serde_json::Value::Null,
        );
        return kit::failed(ctx, reason, RATIO);
    };
    let value_title = block.y.label.clone().unwrap_or_else(|| value_field.clone());
    let readout = block.readout_format(&block.y, ctx.locale);
    let share_format = Format {
        kind: Kind::Percent,
        unit: None,
        digits: block.readout_digits,
        locale: ctx.locale,
    };
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut entries_of: Vec<Vec<(Vec<String>, f64, usize)>> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut entries: Vec<(Vec<String>, f64, usize)> = Vec::new();
        // Which sizes are zero or negative is the exact rows' call: a small one is zero on the grid.
        let skipped = graph::skipped(ctx, fi, members, Some(&value_field));
        for &i in members {
            let sets = rows::text(&ctx.rows[i], &sets_field)
                .map(|t| parse_sets(&t))
                .unwrap_or_default();
            if sets.is_empty() {
                excluded += 1;
                continue;
            }
            match read_number(ctx, "Venn diagram", &mut failures[fi], i, &value_field) {
                Ok(Some(v)) if !skipped.zero.contains(&i) && !skipped.negative.contains(&i) && sets.len() <= 3 => {
                    if sets.len() == 1 && !names.contains(&sets[0]) {
                        names.push(sets[0].clone());
                    }
                    entries.push((sets, v.max(f64::MIN_POSITIVE), i));
                }
                // An overlap of zero is the same as none; it is not an error.
                Ok(Some(_)) if skipped.zero.contains(&i) && (2..=3).contains(&sets.len()) => {}
                Ok(_) => excluded += 1,
                Err(()) => {}
            }
        }
        entries_of.push(entries);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    if excluded > 0 {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} without sets, with a negative size, or with more than three sets {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every row its sets and a size of zero or more."),
            None,
            None,
            json!({"count": excluded}),
        );
        ctx.note.push(format!(
            "{} {} not shown",
            super::count(excluded, "row", "rows"),
            super::verb(excluded, "is", "are")
        ));
    }
    ctx.set_series(&names);
    let mut diagrams: Vec<Option<Diagram>> = Vec::new();
    for (fi, entries) in entries_of.iter().enumerate() {
        if failures[fi].is_some() {
            diagrams.push(None);
            continue;
        }
        match build(entries, "Venn diagram") {
            Ok(d) => diagrams.push(Some(d)),
            Err(reason) => {
                failures[fi] = Some(reason);
                diagrams.push(None);
            }
        }
    }
    let solved: Solutions = ctx.derive_projected(0, "venn.solved", || {
        let mut panels: Vec<Option<Solved>> = diagrams
            .iter()
            .map(|d| {
                d.as_ref().map(|d| match invalid(d, "Venn diagram") {
                    Some(error) => Solved {
                        error: Some(error),
                        centres: Vec::new(),
                        areas: Vec::new(),
                        exact: true,
                    },
                    None => {
                        let (centres, exact) = place(&d.sets, &d.overlaps);
                        Solved {
                            error: None,
                            centres,
                            areas: Vec::new(),
                            exact,
                        }
                    }
                })
            })
            .collect();
        // One unit for the entire author state, including hidden sets, keeps equal areas
        // equal across facets. The unit itself never travels as an unrounded source value.
        let unit = diagrams
            .iter()
            .zip(&panels)
            .filter(|(_, p)| p.as_ref().is_some_and(|s| s.error.is_none()))
            .filter_map(|(d, _)| d.as_ref())
            .flat_map(|d| d.sets.iter().map(|s| s.total))
            .fold(0.0f64, f64::max);
        let unit = if unit > 0.0 { unit } else { 1.0 };
        for (d, solved) in diagrams.iter().zip(&mut panels) {
            if let (Some(d), Some(solved)) = (d, solved)
                && solved.error.is_none()
            {
                solved.areas = d.sets.iter().map(|s| s.total / unit).collect();
                for centre in &mut solved.centres {
                    *centre = centre.map(|p| p / unit.sqrt());
                }
            }
        }
        let areas: Vec<_> = panels.iter().flatten().flat_map(|s| s.areas.iter().copied()).collect();
        let area_grid = normalized_grid(&areas, &share_format);
        let positions: Vec<_> = panels
            .iter()
            .flatten()
            .flat_map(|s| s.centres.iter().flatten().copied())
            .collect();
        let position_format = Format {
            kind: Kind::Number,
            unit: None,
            ..readout.clone()
        };
        let position_grid = statistic_grid(Quant::for_values(
            positions.iter().copied(),
            positions.iter().map(|p| numbers::shown(*p, &position_format)),
        ));
        Solutions {
            panels: panels
                .into_iter()
                .map(|s| {
                    s.map(|s| Solved {
                        error: s.error,
                        centres: s
                            .centres
                            .into_iter()
                            .map(|c| c.map(|p| Position::encode(p, position_grid.as_ref(), &position_format)))
                            .collect(),
                        areas: s
                            .areas
                            .into_iter()
                            .map(|a| Position::Grid(area_grid.index_shown(a, &share_format)))
                            .collect(),
                        exact: s.exact,
                    })
                })
                .collect(),
            area_grid,
            position_grid,
        }
    });
    for (fi, (diagram, panel)) in diagrams.iter_mut().zip(solved.panels).enumerate() {
        if let Some(solved_panel) = panel {
            if let Some(error) = solved_panel.error {
                failures[fi] = Some(error);
                *diagram = None;
            } else if let Some(d) = diagram {
                d.centres = solved_panel
                    .centres
                    .into_iter()
                    .map(|c| c.map(|p| p.at(solved.position_grid.as_ref())))
                    .collect();
                d.radii = solved_panel
                    .areas
                    .into_iter()
                    .map(|a| a.at(Some(&solved.area_grid)).sqrt())
                    .collect();
                d.exact = solved_panel.exact;
            }
        }
    }
    let skipped: usize = diagrams.iter().flatten().map(|d| d.skipped).sum();
    let inexact = diagrams.iter().flatten().filter(|d| !d.exact).count();
    if inexact > 0 {
        ctx.check(
            "points-overlap",
            Level::Warning,
            "The three pair overlaps cannot all be drawn at their sizes, so at least one overlap is drawn off size"
                .to_owned(),
            Some("Give the pair overlaps sizes that three circles can all keep, or read the overlaps from the table."),
            None,
            None,
            json!({"count": inexact}),
        );
    }
    if skipped > 0 {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} an overlap or {} a set with no row of its own, and {} left out",
                super::count(skipped, "overlap row repeats", "overlap rows repeat"),
                super::verb(skipped, "names", "name"),
                super::verb(skipped, "is", "are")
            ),
            Some("Give each set a row of its own and each overlap one row."),
            None,
            None,
            json!({"count": skipped}),
        );
        ctx.note.push(format!(
            "{} {} not shown",
            super::count(skipped, "row", "rows"),
            super::verb(skipped, "is", "are")
        ));
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        ctx.check(
            "source-error",
            Level::Error,
            reason.clone(),
            Some("Give one row for each of two or three sets, and rows for their overlaps."),
            Some("chart.encoding.sets".into()),
            None,
            serde_json::Value::Null,
        );
        return kit::failed(ctx, &reason, RATIO);
    }
    let lh = ctx.line_height();
    let width = ctx.opts.width;
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let ratio = block.style.ratio.unwrap_or(RATIO);
    let panel_h = pw / ratio + title_room;
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let plots: Vec<Rect> = frames
        .iter()
        .map(|f| Rect::new(f.x, f.y + title_room, f.w, (f.h - title_room).max(1.0)))
        .collect();
    let margin = OUTLINE + 2.0;
    // One scale for every panel, so equal totals are equal circles.
    let mut scale = f64::INFINITY;
    for (plot, d) in plots.iter().zip(&diagrams) {
        if let Some(d) = d {
            let e = d.extent();
            if e.w > 0.0 && e.h > 0.0 {
                scale = scale.min(((plot.w - 2.0 * margin) / e.w).min((plot.h - 2.0 * margin) / e.h));
            }
        }
    }
    if !scale.is_finite() || scale <= 0.0 {
        return kit::failed(ctx, "Nothing to draw · The sets are too extreme to represent", RATIO);
    }
    let surface = Style::rgba(ctx.style.surface);
    let ink = ctx.style.ink;
    let mut panels: Vec<Panel> = Vec::new();
    for (fi, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        let Some(d) = &diagrams[fi] else {
            let reason = failures[fi].clone().unwrap_or_default();
            panels.push(super::failed_facet(ctx, *frame, title_room, name, &reason));
            continue;
        };
        let plot = plots[fi];
        let e = d.extent();
        let origin = [
            plot.center()[0] - e.center()[0] * scale,
            plot.center()[1] - e.center()[1] * scale,
        ];
        let px = |c: &[f64; 2]| [origin[0] + c[0] * scale, origin[1] + c[1] * scale];
        let centres: Vec<[f64; 2]> = d.centres.iter().map(px).collect();
        let radii: Vec<f64> = (0..d.sets.len()).map(|k| d.radius(k) * scale).collect();
        // Series index of each set, by name.
        let series: Vec<usize> = d
            .sets
            .iter()
            .map(|s| names.iter().position(|n| *n == s.name).unwrap_or(0))
            .collect();
        let visible: Vec<bool> = series.iter().map(|&s| !ctx.is_hidden(s)).collect();
        let mut items: Vec<Item> = Vec::new();
        let mut labels: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        // Sets first, then overlaps, so the smaller region is the later hit.
        for (k, set) in d.sets.iter().enumerate() {
            if !visible[k] {
                continue;
            }
            let slot = series[k];
            let base = Style::rgba(ctx.series_color(slot));
            let hatch = ctx.series[slot].hatch;
            let others: Vec<([f64; 2], f64)> = (0..d.sets.len())
                .filter(|&o| {
                    // A circle that holds this one whole does not take its name's room.
                    o != k
                        && visible[o]
                        && (centres[o][0] - centres[k][0]).hypot(centres[o][1] - centres[k][1]) + radii[k]
                            > radii[o] + 1e-9
                })
                .map(|o| (centres[o], radii[o]))
                .collect();
            let (anchor, _) = pole(centres[k], radii[k], &others);
            let index = observations.len();
            items.push(Item::Area {
                path: circle_path(centres[k], radii[k]),
                fill: Fill {
                    color: base.with_alpha(SET_ALPHA),
                    hatch,
                    hatch_ink: hatch.map(|_| base),
                    hatch_scale: 1.0,
                },
                role: Role::Mark,
                series: Some(slot),
                obs: Some(index),
            });
            items.push(Item::Line {
                path: circle_path(centres[k], radii[k]),
                stroke: Stroke::solid(base, OUTLINE),
                role: Role::Mark,
                series: Some(slot),
                obs: None,
            });
            let text = ctx.text_mid(
                &set.name,
                anchor[0],
                anchor[1],
                Anchor::Middle,
                Face::Data,
                Role::Label,
                ink,
            );
            if fits(&text.bounds, (centres[k], radii[k]), &others) {
                labels.push(Item::Text(text));
            } else {
                ctx.drop_label(
                    &set.name,
                    DropReason::Density,
                    "set-name",
                    Some(Rect::new(
                        centres[k][0] - radii[k],
                        centres[k][1] - radii[k],
                        2.0 * radii[k],
                        2.0 * radii[k],
                    )),
                );
            }
            observations.push(Observation {
                row: Some(set.row),
                dots: vec![],
                series: Some(slot),
                at: anchor,
                wedge: None,
                area: Some(Rect::new(
                    centres[k][0] - radii[k],
                    centres[k][1] - radii[k],
                    2.0 * radii[k],
                    2.0 * radii[k],
                )),
                radius: 0.0,
                x: anchor[0],
                readout: Readout {
                    heading: set.name.clone(),
                    lines: vec![ReadoutLine {
                        name: value_title.clone(),
                        value: numbers::value(set.total, &readout),
                        series: Some(slot),
                        names_series: false,
                        derived: false,
                    }],
                }
                .into(),
            });
        }
        let shares: Vec<Vec<String>> = ctx.derive(fi, "venn.shares", || {
            d.overlaps
                .iter()
                .map(|o| {
                    o.members
                        .iter()
                        .map(|&k| numbers::value(o.value / d.sets[k].total, &share_format))
                        .collect()
                })
                .collect()
        });
        let mut ordered: Vec<(usize, &Overlap)> = d.overlaps.iter().enumerate().collect();
        ordered.sort_by_key(|(_, o)| o.members.len());
        for (oi, o) in ordered {
            if !o.members.iter().all(|&k| visible[k]) {
                continue;
            }
            let mut shape = circle_polygon(centres[o.members[0]], radii[o.members[0]]);
            for &k in &o.members[1..] {
                shape = clip_convex(&shape, &circle_polygon(centres[k], radii[k]));
            }
            if shape.len() < 3 {
                continue;
            }
            let n = shape.len() as f64;
            let middle = [
                shape.iter().map(|p| p[0]).sum::<f64>() / n,
                shape.iter().map(|p| p[1]).sum::<f64>() / n,
            ];
            let mut path = Path::polyline(&shape);
            path.close();
            let Some(area) = path.bounds() else { continue };
            let index = observations.len();
            items.push(Item::Area {
                path: path.clone(),
                fill: Fill::solid(Style::rgba(ink).with_alpha(LENS_ALPHA)),
                role: Role::Mark,
                series: None,
                obs: Some(index),
            });
            items.push(Item::Line {
                path,
                stroke: Stroke::solid(surface, 1.0),
                role: Role::Mark,
                series: None,
                obs: None,
            });
            let member_names: Vec<&str> = o.members.iter().map(|&k| d.sets[k].name.as_str()).collect();
            let heading = match member_names.as_slice() {
                [a, b] => format!("{a} and {b}"),
                [a, b, c] => format!("{a}, {b} and {c}"),
                other => other.join(", "),
            };
            let mut lines = vec![ReadoutLine {
                name: value_title.clone(),
                value: numbers::value(o.value, &readout),
                series: None,
                names_series: false,
                derived: false,
            }];
            lines.extend(o.members.iter().zip(&shares[oi]).map(|(&k, share)| ReadoutLine {
                name: format!("Share of {}", d.sets[k].name),
                value: share.clone(),
                series: None,
                names_series: false,
                derived: true,
            }));
            observations.push(Observation {
                row: Some(o.row),
                dots: vec![],
                series: None,
                at: middle,
                wedge: None,
                area: Some(area),
                radius: 0.0,
                x: middle[0],
                readout: Readout { heading, lines }.into(),
            });
        }
        items.extend(labels);
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
    ctx.note.push(if inexact > 0 {
        "circle areas are the sets' totals, but the pair overlaps do not all fit together, so some are drawn off size; the overlap of three is where the geometry puts it".into()
    } else {
        "circle areas are the sets' totals and each pair's overlap is drawn at its size; the overlap of three is where the geometry puts it".into()
    });
    ctx.summary = format!(
        "Venn diagram of {}: {}.",
        super::count(names.len(), "set", "sets"),
        names.join(", ")
    );
    let legend = kit::legend(ctx, Swatch::Square);
    Painted {
        height: super::grid_height(facets.len(), columns, panel_h, super::FACET_GAP_Y),
        panels,
        figure_items: vec![],
        legend,
        failure: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_are_split_on_ampersands_and_commas_and_trimmed() {
        assert_eq!(parse_sets("A&B"), ["A", "B"]);
        assert_eq!(parse_sets(" Mobile , Web & Tablet "), ["Mobile", "Web", "Tablet"]);
        assert_eq!(parse_sets("A&A"), ["A"]);
        assert_eq!(parse_sets(r#"["A", "B"]"#), ["A", "B"]);
        assert!(parse_sets(" & ").is_empty());
    }

    #[test]
    fn the_lens_has_the_area_asked_for() {
        for (r1, r2, share) in [(3.0_f64, 2.0_f64, 0.3_f64), (1.0, 1.0, 0.5), (5.0, 1.0, 0.9)] {
            let area = PI * r1.min(r2).powi(2) * share;
            let d = distance_for(r1, r2, area);
            assert!((lens_area(d, r1, r2) - area).abs() < 1e-6 * area, "{r1} {r2} {share}");
        }
        // Nothing shared leaves a gap and everything shared nests the smaller circle.
        assert!(distance_for(2.0, 3.0, 0.0) > 5.0);
        assert!((distance_for(2.0, 3.0, PI * 4.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_cut_lens_has_the_area_of_the_lens() {
        let (a, b) = (circle_polygon([0.0, 0.0], 3.0), circle_polygon([2.5, 0.0], 2.0));
        let cut = clip_convex(&a, &b);
        let area = {
            let n = cut.len();
            (0..n)
                .map(|i| cut[i][0] * cut[(i + 1) % n][1] - cut[(i + 1) % n][0] * cut[i][1])
                .sum::<f64>()
                / 2.0
        };
        let exact = lens_area(2.5, 3.0, 2.0);
        // A 120-sided polygon is within a fraction of a percent of its circle.
        assert!((area - exact).abs() < exact * 0.01, "{area} vs {exact}");
    }

    #[test]
    fn three_sets_are_placed_at_their_pair_distances() {
        let sets = |t: [f64; 3]| -> Vec<SetDef> {
            t.iter()
                .enumerate()
                .map(|(i, &total)| SetDef {
                    name: format!("S{i}"),
                    total,
                    row: i,
                })
                .collect()
        };
        let s = sets([9.0, 9.0, 9.0]);
        let overlaps = vec![
            Overlap {
                members: vec![0, 1],
                value: 3.0,
                row: 3,
            },
            Overlap {
                members: vec![0, 2],
                value: 3.0,
                row: 4,
            },
            Overlap {
                members: vec![1, 2],
                value: 3.0,
                row: 5,
            },
        ];
        let (c, exact) = place(&s, &overlaps);
        assert!(exact);
        let d = |i: usize, j: usize| (c[i][0] - c[j][0]).hypot(c[i][1] - c[j][1]);
        let want = distance_for(3.0, 3.0, PI * 3.0);
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            assert!((d(i, j) - want).abs() < 1e-9);
        }
    }
}
