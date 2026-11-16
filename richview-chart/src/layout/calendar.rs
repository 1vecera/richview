//! Calendar charts: one square per day in week columns, Monday at the top, a strip for each
//! calendar year with its months labelled and outlined, coloured by value on the heatmap's ramp
//! (FEA-CH-75). A strip covers whole months from the first month of the data to the last. A day
//! without a row is an empty outline, never a zero. A key under the strips decodes the colour. A
//! date may appear once per panel, and a calendar spans at most twelve years.

use super::heatmap::{Scale, nice_ceil};
use super::kit::{self, XAxis};
use super::types::Facets;
use super::{Ctx, LABEL_GAP, Painted, Style};
use crate::axis;
use crate::color;
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Hatch, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Stroke,
    Targeting, Text,
};
use crate::numbers::{self, Locale};
use crate::rows::{self, Num};
use crate::time::{self, MS_DAY, Precision};
use serde_json::json;
use std::collections::HashMap;

/// Width over height of the reason a calendar that cannot draw shows.
const RATIO: f64 = 3.0;
/// Largest a day's pitch grows, gap included.
const MAX_PITCH: f64 = 26.0;
const KEY_W: f64 = 200.0;
const KEY_H: f64 = 10.0;
const SWATCH: f64 = 10.0;
const MAX_YEARS: i64 = 12;

/// One day with a value.
struct Day {
    row: usize,
    /// Days since 1970-01-01.
    day: i64,
    value: f64,
}

/// One calendar year as drawn: whole months, from `first` to `last`.
struct Strip {
    year: i64,
    first: u32,
    last: u32,
    start: i64,
    end: i64,
    /// The Monday on or before `start`: the first column begins here.
    monday: i64,
    weeks: usize,
}

fn strips(first: i64, last: i64) -> Vec<Strip> {
    let (y0, m0, _) = time::civil_from_days(first);
    let (y1, m1, _) = time::civil_from_days(last);
    (y0..=y1)
        .map(|year| {
            let a = if year == y0 { m0 } else { 1 };
            let b = if year == y1 { m1 } else { 12 };
            let start = time::days_from_civil(year, a, 1);
            let end = time::days_from_civil(year, b, time::days_in_month(year, b));
            let monday = start - (start + 3).rem_euclid(7);
            Strip {
                year,
                first: a,
                last: b,
                start,
                end,
                monday,
                weeks: ((end - monday).div_euclid(7) + 1) as usize,
            }
        })
        .collect()
}

/// Row of a day in its week column: 0 for Monday.
fn weekday(day: i64) -> usize {
    (day + 3).rem_euclid(7) as usize
}

fn weekday_name(locale: Locale, row: usize) -> &'static str {
    match locale {
        Locale::CsCz => ["Po", "Út", "St", "Čt", "Pá", "So", "Ne"][row % 7],
        Locale::DeDe => ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"][row % 7],
        Locale::JaJp => ["月", "火", "水", "木", "金", "土", "日"][row % 7],
        _ => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][row % 7],
    }
}

/// The fill of a value; a style whose arms share one hue marks the sign with a hatch.
fn value_fill(ctx: &Ctx, scale: &Scale, v: f64) -> Fill {
    let c = scale.color(v);
    let mut fill = Fill::solid(Style::rgba(c));
    if v < 0.0 && ctx.style.sign_hatch {
        let (ink, surface) = (ctx.style.ink, ctx.style.surface);
        fill.hatch = Some(Hatch::Diagonal);
        fill.hatch_ink = Some(Style::rgba(if color::contrast(ink, c) >= color::contrast(surface, c) {
            ink
        } else {
            surface
        }));
    }
    fill
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut panel_days: Vec<Vec<Day>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut days: Vec<Day> = Vec::new();
        let dated: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&i| rows::text(&ctx.rows[i], &x_field).is_some())
            .collect();
        excluded += members.len() - dated.len();
        if dated.is_empty() {
            failures[fi] = Some("Nothing to draw · No row has a date".into());
            panel_days.push(days);
            continue;
        }
        match kit::read_x(ctx, &dated, &x_field) {
            Err(reason) => {
                failures[fi] = Some(format!("Cannot draw this calendar · {reason}"));
            }
            Ok((xs, x_axis)) => {
                if matches!(&x_axis, XAxis::Calendar { precision, .. } if *precision >= Precision::Day) {
                    for &i in &dated {
                        let Some(x) = xs[i] else {
                            excluded += 1;
                            continue;
                        };
                        let value = match rows::num(&ctx.rows[i], &y_field) {
                            Num::Value(v) => v,
                            Num::Missing => {
                                excluded += 1;
                                continue;
                            }
                            Num::Underflow(t) => {
                                ctx.underflow(i, &y_field, &t);
                                failures[fi].get_or_insert_with(|| {
                                    format!("Cannot draw this calendar · {y_field} has a value too small to represent")
                                });
                                continue;
                            }
                            Num::Invalid(t) => {
                                ctx.check(
                                    "non-numeric",
                                    Level::Error,
                                    format!("Row {} has {y_field} '{t}', which is not a number", i + 1),
                                    Some("Write plain numbers; missing values may be empty or NA."),
                                    Some(Ctx::row_path(i, &y_field)),
                                    None,
                                    json!({"row": i}),
                                );
                                failures[fi].get_or_insert_with(|| {
                                    format!("Cannot draw this calendar · {y_field} has text that is not a number")
                                });
                                continue;
                            }
                        };
                        days.push(Day {
                            row: i,
                            day: (x as i64).div_euclid(MS_DAY),
                            value,
                        });
                    }
                } else {
                    ctx.check(
                        "source-error",
                        Level::Error,
                        format!("{x_field} must hold dates with a day, such as 2025-03-14"),
                        Some("Write each date with its year, month and day."),
                        Some("chart.encoding.x".into()),
                        None,
                        json!({}),
                    );
                    failures[fi] = Some("Cannot draw this calendar · x must hold dates with a day".into());
                }
            }
        }
        days.sort_by_key(|d| d.day);
        let repeats: Vec<(usize, i64)> = days
            .windows(2)
            .filter(|w| w[0].day == w[1].day)
            .map(|w| (w[1].row, w[1].day))
            .collect();
        for (row, day) in repeats {
            ctx.check(
                "source-error",
                Level::Error,
                format!(
                    "Row {} repeats the date {}",
                    row + 1,
                    axis::full_date(day * MS_DAY, Precision::Day, ctx.locale)
                ),
                Some("Give each date one row per panel; add the values first if they belong together."),
                Some(Ctx::row_path(row, &x_field)),
                None,
                json!({"row": row}),
            );
            failures[fi].get_or_insert_with(|| "Cannot draw this calendar · A date appears twice".into());
        }
        if failures[fi].is_none()
            && let (Some(a), Some(b)) = (days.first(), days.last())
            && time::civil_from_days(b.day).0 - time::civil_from_days(a.day).0 >= MAX_YEARS
        {
            ctx.check(
                "calendar-range",
                Level::Error,
                format!("The dates span more than {MAX_YEARS} years, more than a calendar draws"),
                Some("Filter the rows to fewer years, or aggregate to months."),
                Some("chart.encoding.x".into()),
                None,
                json!({"years": MAX_YEARS}),
            );
            failures[fi] = Some(format!(
                "Cannot draw this calendar · The dates span over {MAX_YEARS} years"
            ));
        }
        if failures[fi].is_none() && days.is_empty() {
            failures[fi] = Some("Nothing to draw · No row has a date and a value".into());
        }
        panel_days.push(days);
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
                "{} without a date or value {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every row a date and a value."),
            Some("chart.encoding.y".into()),
            None,
            json!({"count": excluded}),
        );
        ctx.note.push(format!(
            "{} without a value {} not shown",
            super::count(excluded, "row", "rows"),
            super::verb(excluded, "is", "are")
        ));
    }
    // One colour scale for every panel, and for every frame of a playback (FEA-BR-31).
    let values: Vec<f64> = panel_days
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .flat_map(|(d, _)| d.iter().map(|x| x.value))
        .collect();
    let wanted = ctx.wanted().color;
    let largest = values
        .iter()
        .chain(wanted.iter().flatten())
        .fold(0.0f64, |m, v| m.max(v.abs()));
    let (lo, hi) = values.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(*v), hi.max(*v))
    });
    ctx.fit.color = lo.is_finite().then(|| super::Fit::widen([lo, hi], wanted));
    // The colour scale's top and whether it runs both ways jump with the data (a grid step moves a
    // value over a round number or across zero), so the exact rows decide them (TYP-36).
    let (top, diverging): (f64, bool) = ctx.derive(0, "calendar.scale", || {
        let diverging = values.iter().chain(wanted.iter().flatten()).any(|v| *v < 0.0);
        (crate::derived::coarse([0.0, nice_ceil(largest)])[1], diverging)
    });
    let scale = Scale::new(&ctx.style, top);
    let top = scale.top;

    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let size = ctx.size();
    let ascent = ctx.style.font.vertical().0;
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let panel_strips: Vec<Vec<Strip>> = panel_days
        .iter()
        .zip(&failures)
        .map(|(d, f)| match (f, d.first(), d.last()) {
            (None, Some(a), Some(b)) => strips(a.day, b.day),
            _ => Vec::new(),
        })
        .collect();
    let year_w = ctx.text_width("0000");
    let name_w = [0usize, 2, 4]
        .iter()
        .map(|&r| ctx.text_width(weekday_name(ctx.locale, r)))
        .fold(0.0, f64::max);
    let gutter = year_w.max(name_w) + LABEL_GAP;
    let max_weeks = panel_strips.iter().flatten().map(|s| s.weeks).max().unwrap_or(1);
    let most_strips = panel_strips.iter().map(Vec::len).max().unwrap_or(1);
    let pitch = ((pw - gutter - 12.0) / max_weeks as f64).clamp(2.0, MAX_PITCH);
    let gap = (pitch * 0.14).clamp(0.5, 3.0);
    let side = pitch - gap;
    let month_row = lh + 4.0;
    let strip_h = month_row + 7.0 * pitch;
    let strip_gap = lh * 0.8;
    let panel_h = title_room + most_strips as f64 * strip_h + most_strips.saturating_sub(1) as f64 * strip_gap;
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let surface = Style::rgba(ctx.style.surface);
    let outline = Style::rgba(ctx.style.ink_faint).with_alpha(0.6);
    let empty_edge = Style::rgba(ctx.style.rule_soft);
    let mut panels: Vec<Panel> = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, title_room, name, reason));
            continue;
        }
        let days = &panel_days[fi];
        let by_day: HashMap<i64, usize> = days.iter().enumerate().map(|(k, d)| (d.day, k)).collect();
        let grid_x = outer.x + gutter;
        let mut items: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        for (k, s) in panel_strips[fi].iter().enumerate() {
            let y0 = outer.y + title_room + k as f64 * (strip_h + strip_gap);
            let grid_y = y0 + month_row;
            let at = |gx: f64, gy: f64| [grid_x + gx * pitch, grid_y + gy * pitch];
            // The year, and the months at the column that holds their first day.
            let year = s.year.to_string();
            let t = ctx.text_mid(
                &year,
                outer.x,
                y0 + lh / 2.0,
                Anchor::Start,
                Face::Figures,
                Role::AxisLabel,
                ctx.style.ink,
            );
            let mut placed: Vec<Rect> = vec![t.bounds];
            let mut month_right = t.bounds.right();
            items.push(Item::Text(Text { weight: 600, ..t }));
            for m in s.first..=s.last {
                let first_day = time::days_from_civil(s.year, m, 1);
                let col = (first_day - s.monday).div_euclid(7);
                let label = axis::month_label(s.year, m, ctx.locale, false);
                let t = ctx.text_mid(
                    &label,
                    at(col as f64, 0.0)[0],
                    y0 + lh / 2.0,
                    Anchor::Start,
                    Face::Data,
                    Role::AxisLabel,
                    ctx.style.ink,
                );
                if t.bounds.x < month_right + 4.0 || t.bounds.right() > outer.right() {
                    ctx.drop_label(&label, DropReason::Density, "month", Some(t.bounds));
                    continue;
                }
                month_right = t.bounds.right();
                items.push(Item::Text(t));
            }
            for row in [2usize, 0, 4] {
                let label = weekday_name(ctx.locale, row);
                let t = ctx.text_mid(
                    label,
                    grid_x - LABEL_GAP / 2.0,
                    at(0.0, row as f64 + 0.5)[1],
                    Anchor::End,
                    Face::Data,
                    Role::AxisLabel,
                    ctx.style.ink_soft,
                );
                if placed.iter().any(|p| p.overlaps(&t.bounds, 1.0)) {
                    ctx.drop_label(label, DropReason::Density, "weekday", Some(t.bounds));
                    continue;
                }
                placed.push(t.bounds);
                items.push(Item::Text(t));
            }
            // The days.
            for d in s.start..=s.end {
                let col = (d - s.monday).div_euclid(7) as f64;
                let r = weekday(d);
                let [x, y] = at(col, r as f64);
                let rect = Rect::new(x + gap / 2.0, y + gap / 2.0, side, side);
                match by_day.get(&d) {
                    Some(&index) => {
                        let day = &days[index];
                        items.push(Item::Rect {
                            rect,
                            fill: value_fill(ctx, &scale, day.value),
                            stroke: None,
                            radius: [(side * 0.12).min(2.0); 4],
                            role: Role::Mark,
                            series: None,
                            obs: Some(observations.len()),
                        });
                        let centre = rect.center();
                        observations.push(Observation {
                            row: Some(day.row),
                            dots: vec![],
                            series: None,
                            at: centre,
                            wedge: None,
                            area: Some(rect),
                            radius: 0.0,
                            x: centre[0],
                            readout: Readout {
                                heading: axis::full_date(d * MS_DAY, Precision::Day, ctx.locale),
                                lines: vec![ReadoutLine {
                                    name: block
                                        .y
                                        .label
                                        .clone()
                                        .unwrap_or_else(|| super::cartesian::readable_name(&y_field)),
                                    value: numbers::value(day.value, &readout),
                                    series: None,
                                    names_series: false,
                                    derived: false,
                                }],
                            }
                            .into(),
                        });
                    }
                    None => items.push(Item::Rect {
                        rect,
                        fill: Fill::solid(surface),
                        stroke: Some(Stroke::solid(empty_edge, 0.75)),
                        radius: [(side * 0.12).min(2.0); 4],
                        role: Role::Missing,
                        series: None,
                        obs: None,
                    }),
                }
            }
            // Each month's outline, along the cell edges.
            for m in s.first..=s.last {
                let f = time::days_from_civil(s.year, m, 1);
                let l = f + i64::from(time::days_in_month(s.year, m)) - 1;
                let (c0, r0) = ((f - s.monday).div_euclid(7) as f64, weekday(f) as f64);
                let (c1, r1) = ((l - s.monday).div_euclid(7) as f64, weekday(l) as f64);
                let mut path = Path::polyline(&[
                    at(c0, r0),
                    at(c0 + 1.0, r0),
                    at(c0 + 1.0, 0.0),
                    at(c1 + 1.0, 0.0),
                    at(c1 + 1.0, r1 + 1.0),
                    at(c1, r1 + 1.0),
                    at(c1, 7.0),
                    at(c0, 7.0),
                ]);
                path.close();
                items.push(Item::Line {
                    path,
                    stroke: Stroke::solid(outline, 1.0),
                    role: Role::Grid,
                    series: None,
                    obs: None,
                });
            }
        }
        let weeks = panel_strips[fi].iter().map(|s| s.weeks).max().unwrap_or(1);
        let plot = Rect::new(
            grid_x,
            outer.y + title_room,
            weeks as f64 * pitch,
            (outer.h - title_room).max(1.0),
        );
        let mut panel = Panel {
            facet: None,
            frame: *outer,
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
    let mut height = super::grid_height(facets.len(), columns, panel_h, super::FACET_GAP_Y);

    // The key: an empty outline for a day without a row, and the ramp from its low to its high.
    let mut figure_items: Vec<Item> = Vec::new();
    let ink_soft = ctx.style.ink_soft;
    let nodata = "No data";
    let nodata_w = ctx.text_width(nodata);
    let lead = SWATCH + 6.0 + nodata_w + 24.0;
    let key_w = KEY_W.min(width - lead - 12.0).max(60.0);
    let x0 = gutter.min(width - lead - key_w).max(0.0);
    let y = height + 14.0;
    figure_items.push(Item::Rect {
        rect: Rect::new(x0, y, SWATCH, KEY_H),
        fill: Fill::solid(surface),
        stroke: Some(Stroke::solid(empty_edge, 0.75)),
        radius: [2.0; 4],
        role: Role::Key,
        series: None,
        obs: None,
    });
    figure_items.push(Item::Text(ctx.text_mid(
        nodata,
        x0 + SWATCH + 6.0,
        y + KEY_H / 2.0,
        Anchor::Start,
        Face::Data,
        Role::Key,
        ink_soft,
    )));
    let kx = x0 + lead;
    let key_lo = if diverging { -top } else { 0.0 };
    let steps = 48;
    for s in 0..steps {
        let t0 = s as f64 / steps as f64;
        let v = key_lo + (top - key_lo) * (t0 + 0.5 / steps as f64);
        figure_items.push(Item::Rect {
            rect: Rect::new(kx + t0 * key_w, y, key_w / steps as f64 + 0.3, KEY_H),
            fill: value_fill(ctx, &scale, v),
            stroke: None,
            radius: [0.0; 4],
            role: Role::Key,
            series: None,
            obs: None,
        });
    }
    let label_y = y + KEY_H + 4.0 + ascent * size;
    let mut ends = vec![(key_lo, kx, Anchor::Start), (top, kx + key_w, Anchor::End)];
    if diverging {
        ends.insert(1, (0.0, kx + key_w / 2.0, Anchor::Middle));
    }
    for (v, x, anchor) in ends {
        figure_items.push(Item::Text(ctx.text(
            &numbers::value(v, &label_format),
            x,
            label_y,
            anchor,
            Face::Figures,
            Role::Key,
            ink_soft,
        )));
    }
    height = label_y + lh / 2.0;

    let all: Vec<&Day> = panel_days
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .flat_map(|(d, _)| d.iter())
        .collect();
    let first = all.iter().map(|d| d.day).min().unwrap_or(0);
    let last = all.iter().map(|d| d.day).max().unwrap_or(0);
    ctx.summary = format!(
        "Calendar of {} from {} to {}; values from {} to {}.",
        super::count(all.len(), "day", "days"),
        axis::full_date(first * MS_DAY, Precision::Day, ctx.locale),
        axis::full_date(last * MS_DAY, Precision::Day, ctx.locale),
        numbers::value(lo, &readout),
        numbers::value(hi, &readout)
    );
    Painted {
        height,
        panels,
        figure_items,
        legend: None,
        failure: None,
    }
}
