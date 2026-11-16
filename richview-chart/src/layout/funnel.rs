//! Funnel charts: stages from top to bottom in source order, each a centred band as wide as its
//! value, joined to the next by a trapezoid. There is no value axis; names stand at the left and
//! values at the right of each band. RichView computes only the shares of the readout, and a
//! negative value fails the panel instead of being drawn (FEA-CH-42, FEA-CH-70).

use super::pie::share_text;
use super::types::Facets;
use super::{Ctx, FACET_GAP_Y, LABEL_GAP, Painted, Style};
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Targeting,
};
use crate::numbers;
use crate::rows::{self, Num};
use serde::{Deserialize, Serialize};
use serde_json::json;

const RATIO: f64 = 4.0 / 3.0;
/// Share of a stage's step taken by its band; the rest is the trapezoid down to the next stage.
const BAND_SHARE: f64 = 0.64;
/// Share of the panel width the names may take at most, and the values.
const NAME_SHARE: f64 = 0.34;
const VALUE_SHARE: f64 = 0.26;
const NAME_LINES: usize = 3;
/// Fewest pixels a stage's step needs, in line heights.
const MIN_STEP: f64 = 1.8;

struct Stage {
    row: usize,
    name: String,
    value: f64,
}

/// What a panel's stages print, written from the exact values: the width of the value column
/// follows the longest text, so the text may not change with the grid (TYP-36).
#[derive(Serialize, Deserialize)]
struct Texts {
    /// The value of each stage on the chart.
    labels: Vec<String>,
    /// Each stage's share of the first stage and of the one before; empty when there is none.
    of_first: Vec<String>,
    of_previous: Vec<String>,
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut stages_by_panel: Vec<Vec<Stage>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut stages = Vec::new();
        for &i in members {
            let row = &ctx.rows[i];
            let name = rows::text(row, &x_field).unwrap_or_else(|| format!("Row {}", i + 1));
            match rows::num(row, &y_field) {
                Num::Value(v) if v < 0.0 => {
                    ctx.check(
                        "source-error",
                        Level::Error,
                        format!(
                            "Row {} has the negative value {}; a stage cannot be negative",
                            i + 1,
                            numbers::value(v, &readout)
                        ),
                        Some("Funnels count what remains at each stage; use a bar chart for signed values."),
                        Some(Ctx::row_path(i, &y_field)),
                        None,
                        json!({"row": i}),
                    );
                    failures[fi]
                        .get_or_insert_with(|| format!("Cannot draw this funnel · {y_field} has a negative value"));
                }
                Num::Value(value) => stages.push(Stage { row: i, name, value }),
                Num::Missing => excluded += 1,
                Num::Underflow(t) => {
                    ctx.underflow(i, &y_field, &t);
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this funnel · {y_field} has a value too small to represent")
                    });
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
                        format!("Cannot draw this funnel · {y_field} has text that is not a number")
                    });
                }
            }
        }
        stages_by_panel.push(stages);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return super::kit::failed(ctx, &reason, RATIO);
    }
    if stages_by_panel.iter().all(Vec::is_empty) {
        return super::kit::failed(ctx, "No stages to show", RATIO);
    }
    if excluded > 0 {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} without a value {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every stage a value."),
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
    ctx.set_series(&[block.y.label.clone().unwrap_or_else(|| y_field.clone())]);
    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let most = stages_by_panel.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let ratio = block.style.ratio.unwrap_or(RATIO);
    let ph = (pw / ratio).max(most as f64 * MIN_STEP * lh + title_room);
    let frames = super::grid_frames(facets.len(), width, 0.0, ph);
    let widest = |stages: &[&Stage], text: &dyn Fn(&Stage) -> String| {
        stages.iter().map(|s| ctx.text_width(&text(s))).fold(0.0f64, f64::max)
    };
    let texts: Vec<Texts> = stages_by_panel
        .iter()
        .enumerate()
        .map(|(fi, stages)| {
            ctx.derive(fi, "funnel.texts", || {
                let first = stages.first().map_or(0.0, |s| s.value);
                let share = |part: f64, whole: f64| {
                    if whole > 0.0 && whole.is_finite() {
                        share_text(part / whole, ctx.locale)
                    } else {
                        String::new()
                    }
                };
                Texts {
                    labels: stages.iter().map(|s| numbers::value(s.value, &label_format)).collect(),
                    of_first: stages.iter().map(|s| share(s.value, first)).collect(),
                    of_previous: stages
                        .iter()
                        .enumerate()
                        .map(|(k, s)| {
                            if k > 0 {
                                share(s.value, stages[k - 1].value)
                            } else {
                                String::new()
                            }
                        })
                        .collect(),
                }
            })
        })
        .collect();
    let all: Vec<&Stage> = stages_by_panel.iter().flatten().collect();
    let name_w = widest(&all, &|s| s.name.clone()).min(pw * NAME_SHARE);
    let value_w = texts
        .iter()
        .flat_map(|t| &t.labels)
        .map(|l| ctx.text_width(l))
        .fold(0.0f64, f64::max)
        .min(pw * VALUE_SHARE);
    let shared_max = all.iter().map(|s| s.value).fold(0.0f64, f64::max);
    let surface = Style::rgba(ctx.style.surface);
    let slot = 0usize;
    let mut panels = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, title_room, name, reason));
            continue;
        }
        let stages = &stages_by_panel[fi];
        if stages.is_empty() {
            panels.push(super::failed_facet(ctx, *outer, title_room, name, "No stages to show"));
            continue;
        }
        let top = outer.y + title_room;
        let body_h = outer.h - title_room;
        let step = body_h / stages.len() as f64;
        let band_h = step * BAND_SHARE;
        let left = outer.x + name_w + LABEL_GAP * 3.0;
        let right = outer.right() - value_w - LABEL_GAP * 3.0;
        let plot = Rect::new(left, top, (right - left).max(1.0), body_h);
        let cx = plot.x + plot.w / 2.0;
        let max = if block.style.free_scales {
            stages.iter().map(|s| s.value).fold(0.0f64, f64::max)
        } else {
            shared_max
        };
        let scale = if max > 0.0 { plot.w / max } else { 0.0 };
        let widths: Vec<f64> = stages.iter().map(|s| (s.value * scale).max(1.0)).collect();
        let color = Style::rgba(ctx.series_color(slot));
        let hatch = ctx.series[slot].hatch;
        let fill = Fill {
            color,
            hatch,
            hatch_ink: hatch.map(|_| surface),
            hatch_scale: 1.0,
        };
        let join = Fill::solid(color.with_alpha(0.35));
        let allowed = (((step - 2.0) / lh).floor() as usize).clamp(1, NAME_LINES);
        let mut items: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        for (k, s) in stages.iter().enumerate() {
            let y = top + k as f64 * step;
            let cy = y + band_h / 2.0;
            if ctx.is_hidden(slot) {
                break;
            }
            if let Some(next) = widths.get(k + 1) {
                let mut path = Path::new();
                path.move_to([cx - widths[k] / 2.0, y + band_h]);
                path.line_to([cx + widths[k] / 2.0, y + band_h]);
                path.line_to([cx + next / 2.0, y + step]);
                path.line_to([cx - next / 2.0, y + step]);
                path.close();
                items.push(Item::Area {
                    path,
                    fill: join,
                    role: Role::Mark,
                    series: Some(slot),
                    obs: None,
                });
            }
            let rect = Rect::new(cx - widths[k] / 2.0, y, widths[k], band_h);
            let r = ctx.style.bar_radius.min(rect.w / 2.0).min(rect.h / 2.0);
            items.push(Item::Rect {
                rect,
                fill,
                stroke: None,
                radius: [r; 4],
                role: Role::Mark,
                series: Some(slot),
                obs: Some(observations.len()),
            });
            // The stage name at the left, on as many lines as the band's step allows.
            let lines = ctx.style.font.wrap(&s.name, ctx.size(), name_w, allowed);
            let fits = lines.len() <= allowed && lines.iter().all(|l| ctx.text_width(l) <= name_w + 0.5);
            if fits {
                for (n, line) in lines.iter().enumerate() {
                    let at = cy + (n as f64 - (lines.len() as f64 - 1.0) / 2.0) * lh;
                    items.push(Item::Text(ctx.text_mid(
                        line,
                        outer.x + LABEL_GAP,
                        at,
                        Anchor::Start,
                        Face::Data,
                        Role::Label,
                        ctx.style.ink,
                    )));
                }
            } else {
                let bounds = Rect::new(outer.x + LABEL_GAP, cy - lh / 2.0, ctx.text_width(&s.name), lh);
                ctx.drop_label(&s.name, DropReason::Density, "name", Some(bounds));
            }
            let value_text = texts[fi].labels[k].clone();
            let t = ctx.text_mid(
                &value_text,
                outer.right() - LABEL_GAP,
                cy,
                Anchor::End,
                Face::Figures,
                Role::Label,
                ctx.style.ink,
            );
            if t.bounds.w <= value_w + 0.5 && t.bounds.inside(outer, 0.5) {
                items.push(Item::Text(t));
            } else {
                ctx.drop_label(&value_text, DropReason::OutOfBounds, "value", Some(t.bounds));
            }
            let mut lines = vec![ReadoutLine {
                name: block.y.label.clone().unwrap_or_else(|| y_field.clone()),
                value: numbers::value(s.value, &readout),
                series: Some(slot),
                names_series: false,
                derived: false,
            }];
            for (name, text) in [
                ("Of the first stage", &texts[fi].of_first[k]),
                ("Of the previous stage", &texts[fi].of_previous[k]),
            ] {
                if !text.is_empty() {
                    lines.push(ReadoutLine {
                        name: name.into(),
                        value: text.clone(),
                        series: None,
                        names_series: false,
                        derived: true,
                    });
                }
            }
            observations.push(Observation {
                row: Some(s.row),
                dots: vec![],
                series: Some(slot),
                at: [cx, cy],
                wedge: None,
                area: Some(rect),
                radius: 0.0,
                x: cx,
                readout: Readout {
                    heading: s.name.clone(),
                    lines,
                }
                .into(),
            });
        }
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
    let labels: Vec<&String> = texts.iter().flat_map(|t| &t.labels).collect();
    ctx.summary = match (all.first(), all.last(), labels.first(), labels.last()) {
        (Some(f), Some(l), Some(fv), Some(lv)) => format!(
            "{} from {} at {} to {} at {}.",
            super::count(all.len(), "stage", "stages"),
            f.name,
            fv,
            l.name,
            lv
        ),
        _ => "No stages.".into(),
    };
    ctx.note
        .push("each band is as wide as its stage's value; shares are computed from the values".into());
    Painted {
        height: super::grid_height(facets.len(), columns, ph, FACET_GAP_Y),
        panels,
        figure_items: Vec::new(),
        legend: None,
        failure: None,
    }
}
