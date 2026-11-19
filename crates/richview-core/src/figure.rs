//! Chart figures (ARC-05, FEA-CH-64, FEA-CH-55). Before the report runtime starts, a figure
//! holds its final box, sized from the geometry model's aspect ratio at the widths readers use,
//! with its title, subtitle, method note and an accessible name and summary in HTML. Without
//! JavaScript or WebGL the box shows a notice. The runtime lays the chart out again from the
//! embedded payload at the exact width the figure occupies. PDF output draws the engine's vector
//! rendering of the same geometry instead (INT-31).

use crate::style::ResolvedStyle;
use crate::{Diagnostic, escape, json_script};
use richview_chart::Level;
use richview_chart::block::{ChartBlock, ChartType};
use richview_chart::geometry::Geometry;
use richview_chart::layout::{ChartTheme, Options, layout};
use richview_chart::numbers::{self, Kind, Locale};
use richview_chart::payload::{ObservationText, Payload, Quant, TextLine, file_name};
use richview_chart::reader_controls::{self, Controls};
use richview_chart::rows;
use serde_json::{Value, json};

/// ARC-05's notice for readers without JavaScript or WebGL.
pub(crate) const NOTICE: &str =
    "This chart cannot be displayed here: it needs JavaScript and WebGL.";

/// The chart section of a resolved style as the engine reads it (T-STYLE-CHART): colours from
/// the style's tokens, palette and ramps from its chart section, chart text in its data face.
pub fn chart_theme(style: &ResolvedStyle) -> ChartTheme {
    let base = ChartTheme::default();
    let token = |name: &str| {
        style
            .custom_properties
            .get(&format!("--rv-{name}"))
            .cloned()
    };
    let chart = &style.chart;
    let colors = |value: Option<&Value>| -> Option<Vec<String>> {
        match value {
            Some(Value::Array(items)) => Some(
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
            ),
            Some(Value::String(name)) => Some(
                richview_chart::rmd_render::palette(name)
                    .iter()
                    .map(|c| (*c).to_owned())
                    .collect(),
            ),
            _ => None,
        }
    };
    let palette = colors(chart.get("palette"))
        .filter(|p| !p.is_empty())
        .unwrap_or(base.palette);
    let three = |key: &str| {
        chart
            .get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .filter(|r| r.len() == 3)
    };
    let text = |key: &str| chart.get(key).and_then(Value::as_str).map(str::to_owned);
    let surface = token("bg-chart")
        .or_else(|| token("bg"))
        .unwrap_or(base.surface);
    let curve = match chart.pointer("/line/smoothing").and_then(Value::as_str) {
        Some("straight") => "linear".to_owned(),
        Some("monotone") => "monotone".to_owned(),
        _ => text("curve").unwrap_or(base.curve),
    };
    ChartTheme {
        page: token("bg").unwrap_or_else(|| surface.clone()),
        surface,
        ink: token("chart-ink")
            .or_else(|| token("ink"))
            .unwrap_or(base.ink),
        ink_soft: token("ink-soft").unwrap_or(base.ink_soft),
        ink_faint: token("ink-faint").unwrap_or(base.ink_faint),
        rule: token("rule").unwrap_or(base.rule),
        // `gridlines` gives the chart's soft rules their own colour (STY-73).
        rule_soft: token("grid")
            .or_else(|| token("rule-soft"))
            .unwrap_or(base.rule_soft),
        accent: token("accent").unwrap_or(base.accent),
        palette,
        ramp: three("ramp").unwrap_or(base.ramp),
        negative_ramp: three("negative-ramp").unwrap_or_default(),
        data_font: text("font-family")
            .or_else(|| token("font-mono"))
            .unwrap_or(base.data_font),
        figures_font: text("font-numeric"),
        ui_font: token("font-ui").unwrap_or(base.ui_font),
        size: base.size,
        bar_radius: chart
            .get("bar-radius")
            .and_then(Value::as_f64)
            .unwrap_or(base.bar_radius),
        decals: chart
            .get("decals")
            .and_then(Value::as_bool)
            .unwrap_or(base.decals),
        flat_glow: chart.get("glow").and_then(Value::as_str) == Some("flat"),
        sign_hatch: chart.get("sign-cue").and_then(Value::as_str) == Some("hatch"),
        curve,
        curvature: chart
            .get("curvature")
            .and_then(Value::as_f64)
            .unwrap_or(base.curvature),
        motion: style
            .document
            .pointer("/motion/enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        motion_profile: motion_profile(style),
        legend: text("legend").unwrap_or(base.legend),
        down: token("down").unwrap_or(base.down),
    }
}

/// The format a column's numbers are rounded to. Positional columns keep the digits that tell
/// their distinct values apart, so rounding never merges two positions.
fn rounding(values: &[f64], format: numbers::Format, positional: bool) -> numbers::Format {
    if !positional {
        return format;
    }
    let distinct = |f: &numbers::Format| {
        let mut v: Vec<f64> = values.iter().map(|x| numbers::shown(*x, f)).collect();
        v.sort_by(f64::total_cmp);
        v.dedup();
        v.len()
    };
    let mut exact = values.to_vec();
    exact.sort_by(f64::total_cmp);
    exact.dedup();
    let mut f = format;
    if f.kind == Kind::Year && distinct(&f) < exact.len() {
        f.kind = Kind::Auto;
    }
    while f.digits < numbers::MAX_DIGITS && distinct(&f) < exact.len() {
        f.digits += 1;
    }
    f
}

/// The payload's columns and rows (TYP-36): the referenced fields plus `encoding.detail`, in
/// source column order where the source recorded it. Numbers are rounded to the precision the
/// author shows in readouts (FEA-CH-37), including numbers written as decimal text in a numeric
/// channel, so the HTML holds no digit a reader is not meant to see. Category and calendar text
/// stays as written.
#[cfg(test)]
pub(crate) fn project(
    block: &ChartBlock,
    rows: &[Value],
    order: &[String],
) -> (Vec<String>, Vec<Vec<Value>>) {
    let p = project_views(std::slice::from_ref(block), &[], rows, order);
    (p.columns, p.shown)
}

/// The payload's rows in the three forms the figure needs (TYP-36).
pub(crate) struct Projection {
    pub columns: Vec<String>,
    /// Numbers at the author's readout precision.
    pub shown: Vec<Vec<Value>>,
    /// The same rows with every numeric channel at its exact source value, for layout.
    pub exact: Vec<Vec<Value>>,
    /// For each numeric channel column, the axis it sits on; the columns of one axis share a
    /// grid. `None` for any other column.
    pub axis: Vec<Option<String>>,
    /// The format each plotted column's numbers are shown in; `None` for a column only the
    /// controls read.
    pub formats: Vec<Option<numbers::Format>>,
}

/// [`project`] over every view a chart's controls can show (FEA-BR-04, FEA-BR-05): each column at
/// the precision of the first view that plots it, and the fields the controls read (`keys`)
/// with the digits that tell their values apart.
pub(crate) fn project_views(
    views: &[ChartBlock],
    keys: &[String],
    rows: &[Value],
    order: &[String],
) -> Projection {
    let mut columns: Vec<String> = Vec::new();
    for c in views
        .iter()
        .flat_map(ChartBlock::referenced_fields)
        .chain(keys.iter().cloned())
    {
        if !columns.contains(&c) {
            columns.push(c);
        }
    }
    if !order.is_empty() {
        columns.sort_by_key(|c| order.iter().position(|o| o == c).unwrap_or(usize::MAX));
    }
    let view_of = |c: &str| {
        views
            .iter()
            .find(|b| b.referenced_fields().iter().any(|f| f == c))
            .unwrap_or(&views[0])
    };
    let is = |field: &Option<String>, c: &str| field.as_deref() == Some(c);
    // Channels whose text is a number: values, bounds, sizes, a heatmap's colour and a numeric x (always numeric for a histogram, density or ECDF).
    let numeric = |c: &str| {
        let block = view_of(c);
        let e = &block.encoding;
        // A heatmap draws its x and y as categories, whatever they hold; its colour is the
        // measure.
        if block.kind == ChartType::Heatmap && (is(&e.x, c) || is(&e.y, c)) {
            return false;
        }
        // A channel that places a mark by a measured value is numeric when every value in it is;
        // identifiers, names and categories travel as written.
        // The shared scale map names x/colour even when they hold categories. Their numeric
        // eligibility is decided below, rather than inferred from decimal-looking identifiers.
        let extra_numeric = block
            .scale_of(c)
            .is_some_and(|scale| !matches!(scale.as_str(), "x" | "color"))
            && rows
                .iter()
                .filter_map(|r| r.get(c))
                .any(|v| !rows::is_missing(v))
            && rows
                .iter()
                .filter_map(|r| r.get(c))
                .filter(|v| !rows::is_missing(v))
                .all(|v| match v {
                    Value::Number(_) => true,
                    Value::String(t) => matches!(rows::parse_decimal(t), Some(Ok(_))),
                    _ => false,
                });
        extra_numeric
            || is(&e.y, c)
            || is(&e.lower, c)
            || is(&e.upper, c)
            || is(&e.size, c)
            || (is(&e.color, c) && block.kind == ChartType::Heatmap)
            || (is(&e.x, c)
                && (block.kind == ChartType::Histogram
                    || matches!(block.kind.name(), "density" | "ecdf")
                    || rows::x_kind(block.kind, c, rows) == rows::XKind::Numeric))
    };
    let number = |v: &Value, text_numbers: bool| -> Option<f64> {
        match v {
            Value::Number(n) => n.as_f64().filter(|v| v.is_finite()),
            Value::String(t) if text_numbers => match rows::parse_decimal(t) {
                Some(Ok(v)) => Some(v),
                _ => None,
            },
            _ => None,
        }
    };
    // A field only the controls read keeps its source value: frames and filter options match it
    // as written.
    let plotted = |c: &str| {
        views
            .iter()
            .any(|b| b.referenced_fields().iter().any(|f| f == c))
    };
    let plans: Vec<Option<(numbers::Format, bool)>> = columns
        .iter()
        .map(|c| {
            if !plotted(c) {
                return None;
            }
            let block = view_of(c);
            let e = &block.encoding;
            let axis = if is(&e.x, c) {
                &block.x
            } else if is(&e.color, c) {
                &block.color
            } else {
                &block.y
            };
            let format = block.readout_format(axis, Locale::EnUs);
            // A `detail` column whose every value is a decimal is a measure shown in readouts,
            // the Table and the CSV: it travels at the author's precision, not as written.
            let decimal_detail = e.detail.iter().any(|d| d == c)
                && rows
                    .iter()
                    .filter_map(|r| r.get(c))
                    .any(|v| !rows::is_missing(v))
                && rows
                    .iter()
                    .filter_map(|r| r.get(c))
                    .filter(|v| !rows::is_missing(v))
                    .all(|v| match v {
                        Value::Number(_) => true,
                        Value::String(t) => matches!(rows::parse_decimal(t), Some(Ok(_))),
                        _ => false,
                    });
            let text_numbers = numeric(c) || decimal_detail;
            let positional = keys.contains(c)
                || is(&e.x, c)
                || is(&e.facet, c)
                || is(&e.label, c)
                || block.is_extra_channel(c);
            let values: Vec<f64> = rows
                .iter()
                .filter_map(|r| r.get(c).and_then(|v| number(v, text_numbers)))
                .collect();
            Some((rounding(&values, format, positional), text_numbers))
        })
        .collect();
    let exact = rows
        .iter()
        .map(|r| {
            columns
                .iter()
                .zip(&plans)
                .map(|(c, plan)| match (r.get(c), plan) {
                    (Some(v), Some((_, true))) => match number(v, true) {
                        Some(n) => json!(n),
                        None if v.is_number() => Value::Null,
                        None => v.clone(),
                    },
                    (Some(v), _) => v.clone(),
                    (None, _) => Value::Null,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    // A field the controls read keeps its value as written, so frames and options still match.
    let axis = plans
        .iter()
        .zip(&columns)
        .map(|(p, c)| {
            if !matches!(p, Some((_, true))) || !numeric(c) || keys.contains(c) {
                return None;
            }
            let block = view_of(c);
            block.scale_of(c)
        })
        .collect();
    let shown = rows
        .iter()
        .map(|r| {
            columns
                .iter()
                .zip(&plans)
                .map(|(c, plan)| match (r.get(c), plan) {
                    (Some(v), None) => v.clone(),
                    (Some(v), Some((format, text_numbers))) => match number(v, *text_numbers) {
                        Some(n) => {
                            let shown = numbers::shown(n, format);
                            if shown.fract() == 0.0 && shown.abs() < 9.0e15 {
                                json!(shown as i64)
                            } else {
                                json!(shown)
                            }
                        }
                        None if v.is_number() => Value::Null,
                        None => v.clone(),
                    },
                    (None, _) => Value::Null,
                })
                .collect()
        })
        .collect();
    Projection {
        columns,
        shown,
        exact,
        axis,
        formats: plans
            .into_iter()
            .map(|p| p.map(|(format, _)| format))
            .collect(),
    }
}

/// What a figure needs from the report around it.
pub(crate) struct Context<'a> {
    pub theme: &'a ChartTheme,
    /// The light counterpart for printing, when the style has one.
    pub print: Option<&'a ChartTheme>,
    pub lang: &'a str,
    pub colors: &'a [(String, usize)],
    /// Phone and desktop column widths, for the box the figure reserves.
    pub widths: [f64; 2],
    /// PDF output: the printed column's width and the page's text height, in CSS pixels.
    pub pdf: Option<[f64; 2]>,
    /// A chart in columns (FEA-RP-68): the width it has once its columns stack on a screen wider
    /// than a phone, for the box the figure reserves there (`--rv-ratio-stacked`).
    pub stacked: Option<f64>,
}

pub(crate) struct Figure {
    pub html: String,
    pub payload: Option<Value>,
    pub diagnostics: Vec<Diagnostic>,
    /// The chart's measured geometry for `check` and the report card.
    pub check: Value,
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::Error => "error",
        Level::Warning => "warning",
        Level::Info => "info",
    }
}

/// The engine's `rows[N].field` as the JSON pointer `/data/inline/N/field` (RFC 6901), the
/// row as the chart's data holds it once resolved.
fn row_pointer(path: &str) -> Option<String> {
    let rest = path.strip_prefix("rows[")?;
    let (row, field) = rest.split_once("].")?;
    let row: usize = row.parse().ok()?;
    Some(format!(
        "/data/inline/{row}/{}",
        field.replace('~', "~0").replace('/', "~1")
    ))
}

fn diagnostics_of(g: &Geometry, id: &str, line: usize, out: &mut Vec<Diagnostic>) {
    for c in &g.checks {
        let mut d = Diagnostic::new(
            level_name(c.level),
            &format!("chart.{}", c.kind),
            c.message.clone(),
            // A check about the chart as a whole sits at the block's root, as the path
            // grammar says; `details.chartId` names which chart (TYP-53).
            c.path.clone().unwrap_or_else(|| "chart".to_owned()),
        )
        .at(line, "chart");
        if let Some(fix) = &c.fix {
            d = d.fix(fix.clone());
        }
        let mut details = json!({"chartId": id, "width": g.width, "check": c});
        // A check on one source row points at that row as written, for editors (ARC-08).
        if let Some(pointer) = c.path.as_deref().and_then(row_pointer) {
            details["path"] = json!(pointer);
        }
        d.details = Some(details);
        if !out
            .iter()
            .any(|e| e.code == d.code && e.message == d.message)
        {
            out.push(d);
        }
    }
    if let Some(reason) = &g.failure
        && !out.iter().any(|e| e.code == "chart.failed")
    {
        let mut d =
            Diagnostic::new("error", "chart.failed", reason.clone(), "chart").at(line, "chart");
        d.details = Some(json!({"chartId": id}));
        out.push(d);
    }
}

/// The chart's measured geometry as `check` reports it (the chart-diagnostics contract): the
/// findings and dropped labels of the layout readers see on a desktop column.
fn check_of(g: &Geometry, id: &str, line: usize) -> Value {
    let checks: Vec<Value> = g
        .checks
        .iter()
        .map(|c| {
            json!({
                "kind": c.kind,
                "severity": level_name(c.level),
                "message": c.message,
                "fix": c.fix,
                "path": c.path,
                "facet": c.facet,
                "region": c.region,
                "details": c.details,
            })
        })
        .collect();
    let dropped: Vec<Value> = g
        .dropped
        .iter()
        .map(|d| {
            let mut v = json!({"text": d.text, "reason": d.reason, "kind": d.kind});
            if let Some(r) = d.would_be_at {
                v["wouldBeAt"] = json!({"x": r.x, "y": r.y, "width": r.w, "height": r.h});
            }
            v
        })
        .collect();
    json!({
        "chartId": id,
        "chartType": g.kind,
        "engine": "rust",
        "measured": true,
        "sourceLine": line,
        "dimensions": {"width": g.width, "height": g.height},
        "summary": g.summary,
        "status": if g.checks.iter().any(|c| c.level != Level::Info) { "issues" } else { "clean" },
        "checks": checks,
        "droppedLabels": dropped,
        "failure": g.failure,
    })
}

/// One chart figure from a validated chart block and its resolved rows.
pub(crate) fn chart(
    value: &Value,
    rows: &[Value],
    order: &[String],
    id: &str,
    line: usize,
    cx: &Context,
) -> Figure {
    let Some(block) = ChartBlock::from_value(value) else {
        let d = Diagnostic::new(
            "error",
            "chart.invalid",
            "The chart block could not be read",
            "chart",
        )
        .at(line, "chart");
        return Figure {
            html: String::new(),
            payload: None,
            diagnostics: vec![d],
            check: json!({"chartId": id, "sourceLine": line}),
        };
    };
    // Reader controls (FEA-BR-01): the payload carries every row and every field each view can
    // show; the figure opens on the initial state.
    let authored = Controls::parse(value, rows);
    let projection = if authored.is_empty() {
        project_views(std::slice::from_ref(&block), &[], rows, order)
    } else {
        let views: Vec<ChartBlock> = authored
            .views(value)
            .iter()
            .filter_map(ChartBlock::from_value)
            .collect();
        let views = if views.is_empty() {
            vec![block.clone()]
        } else {
            views
        };
        project_views(&views, &authored.fields(value), rows, order)
    };
    // Numeric channels travel on a 4,096-step grid per column and, for the Table and the CSV,
    // at the author's precision; layout runs on the exact rows (TYP-36).
    let Projection {
        columns,
        shown,
        exact,
        axis,
        formats,
    } = projection;
    let mut quant = std::collections::BTreeMap::new();
    let mut shown_columns = std::collections::BTreeMap::new();
    // Nothing exact reaches the page: a plotted column off the grid carries its author-precision
    // value, and one grid serves every column of an axis, so equal values stay equal.
    let mut projected = shown.clone();
    let mut grids: std::collections::BTreeMap<String, Quant> = std::collections::BTreeMap::new();
    for kind in axis
        .iter()
        .flatten()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let grid = Quant::for_values(
            columns
                .iter()
                .enumerate()
                .filter(|(i, _)| axis[*i].as_ref() == Some(kind))
                .flat_map(|(at, _)| exact.iter().filter_map(move |r| r[at].as_f64())),
            columns
                .iter()
                .enumerate()
                .filter(|(i, _)| axis[*i].as_ref() == Some(kind))
                .flat_map(|(at, _)| shown.iter().filter_map(move |r| r[at].as_f64())),
        );
        if let Some(grid) = grid {
            grids.insert(kind.clone(), grid);
        }
    }
    for (at, name) in columns.iter().enumerate() {
        let Some(grid) = axis[at].as_ref().and_then(|kind| grids.get(kind)) else {
            continue;
        };
        // A value shown coarser than a refined grid's step sits no finer than it shows.
        let format = formats[at].clone().unwrap_or_default();
        for (row, exact_row) in projected.iter_mut().zip(&exact) {
            if let Some(n) = exact_row[at].as_f64() {
                row[at] = json!(grid.index_shown(n, &format));
            }
        }
        shown_columns.insert(
            name.clone(),
            shown.iter().map(|r| r[at].clone()).collect::<Vec<_>>(),
        );
        quant.insert(name.clone(), grid.clone());
    }
    // The payload carries the block without its rows; the rows travel once, projected.
    let mut embedded = value.clone();
    if let Some(map) = embedded.as_object_mut() {
        map.remove("data");
    }
    // `wide: true` takes the full-width class the breakout rule keys on (FEA-BR-18).
    let full = if block.wide { " rv-chart--full" } else { "" };
    let payload = Payload {
        id: id.to_owned(),
        block: embedded,
        columns,
        rows: projected,
        quant,
        shown: shown_columns,
        text: Vec::new(),
        labels: Vec::new(),
        derived: Vec::new(),
        theme: cx.theme.clone(),
        print: cx.print.filter(|p| *p != cx.theme).cloned(),
        lang: cx.lang.to_owned(),
        colors: cx.colors.to_vec(),
        file: file_name(&block.title),
    };
    // Layout and checks run on the exact rows; only readers' text is rounded (TYP-36).
    let objects = Payload {
        rows: exact,
        ..payload.clone()
    }
    .objects();
    let controls = Controls::parse(value, &objects);
    let initial = controls.initial();
    // The state laid out at `width`; the initial state unless another is asked for.
    let state_at = |state: &reader_controls::State, width: f64, theme: &ChartTheme| {
        let options = Options {
            width,
            lang: cx.lang.to_owned(),
            colors: cx.colors.to_vec(),
            grids: payload.quant.clone(),
            ..Options::default()
        };
        if controls.is_empty() {
            return layout(&block, &objects, theme, &options);
        }
        let options = controls.options(value, &objects, state, theme, &options);
        match controls.chart(value, &objects, state) {
            Some(r) => layout(&r.block, &r.rows, theme, &options),
            None => layout(&block, &objects, theme, &options),
        }
    };
    let at = |width: f64, theme: &ChartTheme| state_at(&initial, width, theme);
    // The state in words, for print, PDF and downloads (FEA-BR-02 to FEA-BR-05, FEA-BR-32).
    let state_line = if controls.is_empty() {
        String::new()
    } else {
        format!(
            "<p class=\"rv-chart__state\">{}</p>",
            escape(&controls.describe(&initial))
        )
    };
    // The title row, whose right end holds the action group (FEA-CH-43; Daniel, 9 Nov), then
    // the subtitle. The group is reserved here so the runtime fills it without moving the page.
    let head = |actions: bool, zooms: bool| {
        format!(
            "<figcaption class=\"rv-chart__head\"><div class=\"rv-chart__titlerow\"><p class=\"rv-chart__title\">{}</p>{}</div>{}</figcaption>",
            escape(&block.title),
            if actions {
                // The buttons take their final space but stay invisible and inert until the
                // runtime wires them; without it they never show (ARC-05).
                format!(
                    "<div class=\"rv-chart__actions\" role=\"group\" aria-label=\"Chart actions\" data-rv-pending inert>{}</div>",
                    richview_chart::interaction::ACTIONS
                        .iter()
                        .map(|(kind, label)| richview_chart::interaction::action_html(kind, label))
                        .collect::<String>()
                        // A chart a mouse can zoom keeps Reset zoom's place from the start, so
                        // the title beside the icons wraps the same with the runtime, without
                        // it and when a zoom starts (Chart design, 16 November).
                        + &if zooms {
                            richview_chart::interaction::action_html("reset", "Reset zoom")
                                .replacen("<button ", "<button data-rv-place ", 1)
                        } else {
                            String::new()
                        }
                )
            } else {
                String::new()
            },
            block
                .subtitle
                .as_deref()
                .map(|s| format!("<p class=\"rv-chart__subtitle\">{}</p>", escape(s)))
                .unwrap_or_default(),
        )
    };
    let method = |g: &Geometry| {
        g.note
            .as_deref()
            .map(|n| format!("<p class=\"rv-chart__method\">{}</p>", escape(n)))
            .unwrap_or_default()
    };
    let label = |g: &Geometry| escape(&format!("{}. {}", block.title, g.summary));
    let mut diagnostics = Vec::new();
    if let Some([width, page]) = cx.pdf {
        let mut theme = cx.print.unwrap_or(cx.theme).clone();
        theme.motion = false;
        let mut g = at(width, &theme);
        // A chart prints whole and at most 60% of the page high (FEA-CH-67). A taller one is
        // laid out narrower at the same type size, rather than scaled down, so its text stays
        // the size of the rest of the chart text on paper.
        let cap = page * PRINT_HEIGHT_SHARE;
        for _ in 0..4 {
            if g.height <= cap {
                break;
            }
            let narrower = (g.width * cap / g.height).floor().max(MIN_PRINT_WIDTH);
            if narrower >= g.width {
                break;
            }
            let next = at(narrower, &theme);
            if next.height >= g.height {
                break;
            }
            g = next;
        }
        if g.height > cap + 0.5 {
            let mut d = Diagnostic::new(
                "warning",
                "chart.print-height",
                format!(
                    "The chart is {:.0} px tall on paper, over the {:.0}% of the page height it may take, even at its narrowest layout",
                    g.height,
                    PRINT_HEIGHT_SHARE * 100.0
                ),
                "chart",
            )
            // The fix is the registry's: fewer rows, or a wider ratio, which prints shorter.
            .at(line, "chart");
            d.details = Some(json!({"chartId": id}));
            diagnostics.push(d);
        }
        diagnostics_of(&g, id, line, &mut diagnostics);
        let svg = richview_chart::vector::svg_for_pdf(&g, &theme, id);
        // Legend entries print as plain text, on the side the style places them (FEA-CH-67,
        // FEA-CH-33): the vector chart names no series a legend names.
        let legend = legend_html(&g, "");
        let (above, below) = if g.legend.as_ref().is_some_and(|l| l.bottom) {
            ("", legend.as_str())
        } else {
            (legend.as_str(), "")
        };
        return Figure {
            html: format!(
                "<figure class=\"rv-chart{full} rv-chart--print\" id=\"{id}\">{}{state_line}{above}<div class=\"rv-chart__frame\" role=\"img\" aria-label=\"{}\">{svg}</div>{below}{}</figure>",
                head(false, false),
                label(&g),
                method(&g)
            ),
            payload: None,
            diagnostics,
            check: check_of(&g, id, line),
        };
    }
    let [phone, desktop] = cx.widths.map(|w| at(w, cx.theme));
    let stacked = cx.stacked.map(|w| at(w, cx.theme));
    diagnostics_of(&desktop, id, line, &mut diagnostics);
    diagnostics_of(&phone, id, line, &mut diagnostics);
    // What each state's layout from the exact rows derived, for the runtime to reuse (TYP-36).
    let mut derived_states = vec![richview_chart::payload::StateDerived {
        state: if controls.is_empty() {
            vec![]
        } else {
            initial.clone()
        },
        derived: desktop.derived.clone(),
    }];
    // Every reachable state checks clean or says which state fails (FEA-BR-33).
    if !controls.is_empty() {
        let (states, total) = controls.states(reader_controls::STATE_LIMIT);
        let mut found: Vec<(Diagnostic, String, usize)> = Vec::new();
        for state in states.iter().filter(|s| **s != initial) {
            let mut own = Vec::new();
            let g = state_at(state, cx.widths[1], cx.theme);
            diagnostics_of(&g, id, line, &mut own);
            derived_states.push(richview_chart::payload::StateDerived {
                state: state.clone(),
                derived: g.derived,
            });
            for d in own {
                if diagnostics
                    .iter()
                    .any(|e| e.code == d.code && e.message == d.message)
                {
                    continue;
                }
                match found
                    .iter_mut()
                    .find(|(e, _, _)| e.code == d.code && e.message == d.message)
                {
                    Some((_, _, more)) => *more += 1,
                    None => found.push((d, controls.describe(state), 0)),
                }
            }
        }
        for (mut d, state, more) in found {
            d.message = match more {
                0 => format!("{} (state {state})", d.message),
                n => format!("{} (state {state} and {n} more)", d.message),
            };
            diagnostics.push(d);
        }
        if total > reader_controls::STATE_LIMIT {
            diagnostics.push(
                Diagnostic::new(
                    "warning",
                    "chart.states-unchecked",
                    format!(
                        "{} of this chart's {total} states were not checked; checks cover the first {}",
                        total - reader_controls::STATE_LIMIT,
                        reader_controls::STATE_LIMIT
                    ),
                    id,
                )
                .at(line, "chart"),
            );
        }
    }
    let ratio = |g: &Geometry| format!("{:.4}", g.width / g.height.max(1.0));
    // The readouts the exact rows give, one set per control state the checks cover, for the
    // runtime to show in place of any it would format from the grid (TYP-36).
    let mut payload = payload;
    // On-chart value texts depend on the style's type and so on the layout; they are not
    // embedded, which keeps the payload one thing for every style (the engine keys them by mark).
    let text_of = |g: &Geometry, state: &[usize], out: &mut Vec<ObservationText>| {
        let mut ids = g.mark_ids().into_iter();
        for (panel, p) in g.panels.iter().enumerate() {
            for (obs, o) in p.observations.iter().enumerate() {
                out.push(ObservationText {
                    state: state.to_vec(),
                    panel,
                    obs,
                    id: ids.next().unwrap_or_default(),
                    heading: o.readout.heading.clone(),
                    lines: o
                        .readout
                        .lines
                        .iter()
                        .map(|l| TextLine {
                            name: l.name.clone(),
                            value: l.value.clone(),
                            derived: l.derived,
                            series: l.series,
                            names_series: l.names_series,
                        })
                        .collect(),
                });
            }
        }
    };
    payload.derived = derived_states
        .into_iter()
        .filter(|d| !d.derived.is_empty())
        .collect();
    // The initial state only: other states take their text from the shown values (TYP-36).
    text_of(
        &desktop,
        if controls.is_empty() { &[] } else { &initial },
        &mut payload.text,
    );
    let payload = serde_json::to_value(&payload).unwrap_or(Value::Null);
    // The legend each reference width draws, so neither moves the page when the runtime swaps in
    // its own; one copy when they agree (FEA-CH-33, FEA-CH-64).
    let bottom = |g: &Geometry| g.legend.as_ref().is_some_and(|l| l.bottom);
    let (mut above, mut below) = (String::new(), String::new());
    let same =
        legend_html(&desktop, "") == legend_html(&phone, "") && bottom(&desktop) == bottom(&phone);
    let variants: &[(&Geometry, &str)] = if same {
        &[(&desktop, "")]
    } else {
        &[
            (&desktop, " rv-chart__legend--desktop"),
            (&phone, " rv-chart__legend--phone"),
        ]
    };
    for (g, class) in variants {
        let html = legend_html(g, class);
        if bottom(g) {
            below.push_str(&html)
        } else {
            above.push_str(&html)
        }
    }
    let html = format!(
        "<figure class=\"rv-chart{full}\" id=\"{id}\" data-rv-chart>{}{}{state_line}{above}<div class=\"rv-chart__frame\" role=\"img\" aria-label=\"{}\" style=\"--rv-ratio:{};--rv-ratio-phone:{}{}\"><p class=\"rv-chart__notice\">{NOTICE}</p></div>{below}{}<script type=\"application/json\" data-rv-payload>{}</script></figure>",
        head(
            true,
            richview_chart::interaction::zooms(block.kind.name(), &desktop)
        ),
        if controls.is_empty() {
            String::new()
        } else {
            controls.html(&initial, id)
        },
        label(&desktop),
        ratio(&desktop),
        ratio(&phone),
        stacked
            .as_ref()
            .map(|g| format!(";--rv-ratio-stacked:{}", ratio(g)))
            .unwrap_or_default(),
        method(&desktop),
        json_script(&payload),
    );
    Figure {
        html,
        payload: Some(payload),
        diagnostics,
        check: check_of(&desktop, id, line),
    }
}

/// The legend in HTML before the runtime starts (FEA-CH-33, STY-40 swatches); the runtime turns
/// each entry into a toggle of the same size.
fn legend_html(g: &Geometry, class: &str) -> String {
    let Some(legend) = &g.legend else {
        return String::new();
    };
    let swatch = match legend.swatch {
        richview_chart::geometry::Swatch::Square => "square",
        richview_chart::geometry::Swatch::Line => "line",
        richview_chart::geometry::Swatch::Dot => "dot",
    };
    let entries: String = legend
        .entries
        .iter()
        .filter_map(|&i| g.series.get(i).map(|s| (i, s)))
        .map(|(i, s)| {
            // A series told apart by hatch, dash or marker shows it here too (FEA-CH-57).
            let (cue, art) = richview_chart::vector::legend_swatch(g, i, legend.swatch)
                .map_or((String::new(), String::new()), |svg| (" rv-chart__swatch--cue".to_owned(), svg));
            format!(
                "<span class=\"rv-chart__legend-entry\"><span class=\"rv-chart__swatch rv-chart__swatch--{swatch}{cue}\" style=\"--rv-swatch:{}\">{art}</span><span class=\"rv-chart__legend-name\">{}</span></span>",
                s.color.hex(),
                escape(&s.name)
            )
        })
        .collect();
    format!(
        "<div class=\"rv-chart__legend{class}\" role=\"group\" aria-label=\"Series\" data-rv-static>{entries}</div>"
    )
}

/// Figure styles: the reserved box, the notice, the runtime's controls and the readout
/// (STY-39 faces, STY-40 swatches).
/// The share of the page's text height a printed chart may take (FEA-CH-67).
const PRINT_HEIGHT_SHARE: f64 = 0.6;
/// The narrowest a printed chart is laid out to fit that height; the engine's own minimum.
const MIN_PRINT_WIDTH: f64 = 240.0;

/// An icon-only action's name under it, in the readout's raised surface, hairline and face
/// (FEA-CH-42), square and casting no shadow, which STY-36 and STY-37 keep for the readout and
/// menus; hidden until hover or keyboard focus. Opacity keeps it the button's accessible name,
/// and it takes no pointer (S27-876).
macro_rules! action_name {
    () => {
        "position:absolute;top:calc(100% + 6px);right:0;z-index:5;box-sizing:border-box;padding:4px 8px;white-space:nowrap;font:12px/1.35 var(--rv-font-ui,system-ui);color:var(--rv-ink);background:var(--rv-bg-elev,var(--rv-bg));border:1px solid var(--rv-rule);opacity:0;pointer-events:none"
    };
}

pub(crate) const FIGURE_CSS: &str = concat!(
    ".rv-chart{position:relative}",
    ".rv-chart__head{display:block;margin-bottom:.5rem}",
    // The title row holds the title and, at its right end, the action group (FEA-CH-43). The title
    // is set a tenth under the base size (Daniel, 9 Nov), still above the subtitle's small size.
    // A row wide enough for the group beside 15rem of title keeps the group there, and a long title
    // wraps beside it; a narrower figure on a wide screen sends the group under the title, where the
    // row gap undoes the group's lift to the title's first line (S27-996; Daniel, 9 Nov: icons in
    // the title row if they fit, otherwise under the title).
    ".rv-chart__titlerow{display:flex;flex-wrap:wrap;align-items:flex-start;gap:.375rem 1rem}",
    // Shift only the painted title: a negative margin changes its flex width and can rewrap it
    // when the runtime measures its first glyph, moving the chart after hydration.
    ".rv-chart .rv-chart__title{flex:1 1 15rem;min-width:0;margin:0 0 .25rem;transform:translateX(calc(-1 * var(--rv-title-bearing,0px)));font:600 calc(var(--rv-size-base,1.125rem) * .9)/1.25 var(--rv-font-display,inherit)}",
    // A phone has no room for four icons beside a title and no line to give them (Daniel, 17 Nov:
    // "drop download image, download table icons, make sure there is no empty line for icons"). Up
    // to 600 px, where the chart takes its phone ratio, a figure shows Table and Expand alone;
    // Download PNG and CSV are in the expanded view (FEA-CH-44). The row never wraps there: the two
    // icons keep the right end of the title's first line and a long title wraps beside them, so
    // title, subtitle and plot follow each other, before the runtime wires the icons and without it.
    "@media (max-width:600px){.rv-chart__titlerow{flex-wrap:nowrap}.rv-chart__titlerow>.rv-chart__title{flex-basis:0}.rv-chart__titlerow>.rv-chart__actions>.rv-chart__action--png,.rv-chart__titlerow>.rv-chart__actions>.rv-chart__action--csv{display:none}}",
    ".rv-chart__subtitle{margin:0;font:var(--rv-size-small,14px)/1.45 var(--rv-font-text,inherit);color:var(--rv-ink-soft)}",
    ".rv-chart__frame{position:relative;width:100%;aspect-ratio:var(--rv-ratio,1.6);background:var(--rv-bg-chart,transparent)}",
    "@media (max-width:600px){.rv-chart__frame{aspect-ratio:var(--rv-ratio-phone,var(--rv-ratio,1.2))}.rv-chart__legend.rv-chart__legend--desktop{display:none}}",
    "@media (min-width:601px){.rv-chart__legend.rv-chart__legend--phone{display:none}}",
    // The focused chart's ring (STY-38): 2 px in the link colour, the accent where the style's
    // links are accent and ink in Hand-drawn. It is drawn by the frame itself so each side can
    // keep 3 px from the chart's text: the runtime sets `--rv-ring-*` from the geometry's text and
    // the text above and below the frame, and a side it has not placed sits 4 px outside. While an
    // arrow key has a mark selected, the canvas rings that mark instead (`data-rv-selected`).
    ".rv-chart__frame::after{position:absolute;inset:var(--rv-ring-top,-6px) var(--rv-ring-right,-6px) var(--rv-ring-bottom,-6px) var(--rv-ring-left,-6px);border:2px solid var(--rv-link,var(--rv-accent));pointer-events:none}",
    "figure.rv-chart .rv-chart__frame:focus-visible{outline:none}",
    "figure.rv-chart .rv-chart__frame:focus-visible:not([data-rv-selected])::after{content:\"\"}",
    // In the expanded view the chart's ring follows the reader's input, not each engine's guess:
    // after a pointer opened the dialog the chart holds focus without it, as Chromium and Firefox
    // already show it; once a key is pressed in the dialog, or after a keyboard open, the focused
    // chart shows it in every engine, Firefox included, which keeps a pointer's focus unringed
    // after keys (S27-878).
    ".rv-chart-dialog[data-rv-pointer] figure.rv-chart .rv-chart__frame::after{content:none}",
    ".rv-chart-dialog:not([data-rv-pointer]) .rv-chart__frame:focus{outline:none}",
    ".rv-chart-dialog:not([data-rv-pointer]) .rv-chart__frame:focus:not([data-rv-selected])::after{content:\"\"}",
    ".rv-chart__frame>svg{display:block;width:100%;height:auto}",
    // A printed chart keeps the width it was laid out at, centred when it was narrowed.
    // Paper reserves no box: the frame is as tall as the drawing, so a narrowed chart leaves no blank band.
    ".rv-chart--print .rv-chart__frame{aspect-ratio:auto}",
    ".rv-chart--print .rv-chart__frame>svg{width:auto;max-width:100%;margin-inline:auto}",
    ".rv-chart__canvas{display:block;touch-action:pan-y pinch-zoom;-webkit-user-select:none;user-select:none}",
    // Touch pinches and pans the chart only in the expanded view (FEA-CH-49).
    ".rv-chart--expanded .rv-chart__canvas{touch-action:none}",
    // The overview strip after a zoom (FEA-CH-50).
    ".rv-chart__overview{position:relative;height:2.25rem;margin:.5rem 0 0;border-bottom:1px solid var(--rv-rule-soft,var(--rv-rule));touch-action:none;cursor:pointer}",
    ".rv-chart__overview>svg{position:absolute;inset:0;width:100%;height:100%;overflow:visible}",
    ".rv-chart__overview-window{position:absolute;top:-1px;bottom:-1px;min-width:4px;box-sizing:border-box;border:1px solid var(--rv-accent);border-radius:0;background:color-mix(in oklch,var(--rv-accent) 10%,transparent);cursor:grab}",
    ".rv-chart__overview-window:active{cursor:grabbing}",
    ".rv-chart__overview-window:focus-visible{outline:2px solid var(--rv-accent);outline-offset:2px}",
    "@media print{.rv-chart__overview{display:none}}",
    ".rv-chart__notice{position:absolute;inset:0;display:flex;align-items:center;justify-content:center;margin:0;padding:1rem;text-align:center;color:var(--rv-ink-soft);font:var(--rv-size-small,14px)/1.45 var(--rv-font-ui,system-ui);border:1px dashed var(--rv-rule)}",
    ".rv-chart__notice[hidden]{display:none}",
    // The loading state (ARC-05, docs/MOTION.md). While scripts run and a chart is not drawn yet,
    // its reserved box shows the site's tiny bar chart (STY-45) at its centre in the chart's soft
    // ink, rising and falling; it fades in only when drawing takes longer than 300 ms, and the
    // runtime's first frame replaces it. The notice is for readers without JavaScript or WebGL:
    // it shows when scripting is off, when the runtime finds it cannot draw (`data-rv-notice`),
    // and when the runtime has not started after 10 s.
    "@media (scripting:enabled){",
    ".rv-chart:not(.rv-chart--print)>.rv-chart__frame:not([data-rv-painted],[data-rv-notice])::before{content:\"\";position:absolute;left:50%;top:50%;width:22px;height:18px;margin:-9px 0 0 -11px;color:var(--rv-ink-soft);background:linear-gradient(currentColor,currentColor) 0 100%/4px 40% no-repeat,linear-gradient(currentColor,currentColor) 50% 100%/4px 70% no-repeat,linear-gradient(currentColor,currentColor) 100% 100%/4px 100% no-repeat;opacity:0;pointer-events:none;animation:rv-chart-loading-in 200ms cubic-bezier(.33,1,.68,1) 300ms forwards,rv-chart-loading .8s cubic-bezier(.4,0,.2,1) 300ms infinite alternate}",
    "html:not([data-rv-runtime]) .rv-chart:not(.rv-chart--print)>.rv-chart__frame:not([data-rv-painted],[data-rv-notice])::before{animation:rv-chart-loading-in 200ms cubic-bezier(.33,1,.68,1) 300ms forwards,rv-chart-loading .8s cubic-bezier(.4,0,.2,1) 300ms infinite alternate,rv-chart-late-out 1ms 10s forwards}",
    ".rv-chart__frame:not([data-rv-notice])>.rv-chart__notice{visibility:hidden}",
    "html:not([data-rv-runtime]) .rv-chart__frame>.rv-chart__notice{animation:rv-chart-late-in 1ms 10s forwards}",
    "}",
    "@keyframes rv-chart-loading-in{to{opacity:.6}}",
    "@keyframes rv-chart-loading{from{background-size:4px 30%,4px 90%,4px 50%}to{background-size:4px 90%,4px 40%,4px 100%}}",
    "@keyframes rv-chart-late-in{to{visibility:visible}}",
    "@keyframes rv-chart-late-out{to{visibility:hidden}}",
    // Reduced motion keeps the tiny chart still.
    "@media (prefers-reduced-motion:reduce){.rv-chart__frame::before{animation-iteration-count:1,0,1!important}}",
    ".rv-chart__method{margin:.5rem 0 0;color:var(--rv-ink-soft);font:var(--rv-size-small,14px)/1.45 var(--rv-font-text,inherit)}",
    ".rv-chart__actions{display:flex;flex-wrap:nowrap;gap:.25rem;margin-left:auto;min-height:2rem}",
    // The icons' centre is the title's first line's: half of its 1.25 line at .9 of the base size.
    ".rv-chart__titlerow>.rv-chart__actions{margin-top:calc(.5625 * var(--rv-size-base,1.125rem) - 16px)}",
    ".rv-chart__actions[data-rv-pending]{visibility:hidden}",
    // Reset zoom's reserved place: kept where a mouse can zoom, gone on a touch screen, where
    // zoom is in the expanded view; the runtime takes the button over.
    ".rv-chart__action--reset[data-rv-place]{visibility:hidden}",
    "@media not (any-pointer:fine){.rv-chart__action--reset[data-rv-place]{display:none}}",
    ".rv-chart__action{position:relative;display:inline-flex;align-items:center;justify-content:center;gap:.4rem}",
    ".rv-chart__action svg{width:18px;height:18px;flex:none;stroke-width:1.5}",
    // Quiet buttons (STY-42): the UI face in `ink-soft`, turning `ink` on hover and focus; a pressed toggle is underlined, never filled.
    ".rv-chart__action{font:var(--rv-size-small,14px)/1.2 var(--rv-font-ui,system-ui);color:var(--rv-ink-soft);background:transparent;border:0;border-radius:0;padding:.3rem .5rem;min-height:32px;min-width:32px;cursor:pointer}",
    ".rv-chart__action:hover,.rv-chart__action:focus-visible{color:var(--rv-ink)}",
    ".rv-chart__action:focus-visible{outline:2px solid var(--rv-accent);outline-offset:2px}",
    ".rv-chart__action[aria-pressed=true],.rv-chart__action[aria-expanded=true]{color:var(--rv-ink);text-decoration:underline;text-underline-offset:.25em;text-decoration-thickness:1.5px}",
    ".rv-chart__action[hidden]{display:none}",
    // The actions and Close are icons alone at every width (FEA-CH-43; Daniel, 9 Nov): the label is
    // the accessible name and shows as the action's name on hover and focus (S27-876). Pressed,
    // Table's icon is underlined by a bar under it. Buttons that have words and no icon, as the
    // Table's "Show all", keep their words (S27-877).
    ".rv-chart__actions .rv-chart__action,.rv-chart-dialog__close{font-size:0;gap:0;padding:.3rem}",
    ".rv-chart__actions .rv-chart__action[aria-pressed=true]{background:linear-gradient(currentColor,currentColor) 50% calc(50% + 12px)/18px 1.5px no-repeat}",
    ".rv-chart__legend{display:flex;flex-wrap:wrap;gap:.125rem .875rem;margin:0 0 .375rem}",
    ".rv-chart__legend-entry{display:inline-flex;align-items:center;gap:.4rem;margin:0;font:12px/1.3 var(--rv-font-ui,system-ui);color:var(--rv-ink);background:none;border:0;padding:.3rem 0;min-height:1.5rem}",
    "button.rv-chart__legend-entry{cursor:pointer}",
    ".rv-chart__legend-name{color:var(--rv-ink-soft)}.rv-chart__legend-entry:hover .rv-chart__legend-name{color:var(--rv-ink)}",
    ".rv-chart__legend-entry:focus-visible{outline:2px solid var(--rv-accent);outline-offset:2px}",
    ".rv-chart__legend-entry[aria-pressed=false]{opacity:.45;text-decoration:line-through}",
    ".rv-chart__swatch{display:inline-block;flex:none;width:9px;height:9px;background:var(--rv-swatch)}",
    ".rv-chart__swatch--line{width:14px;height:2px}",
    ".rv-chart__swatch--dot{border-radius:50%}",
    // A swatch that carries its series' hatch, dash or marker is that drawing (FEA-CH-57).
    ".rv-chart__swatch--cue{width:auto;height:auto;background:none;border-radius:0}",
    ".rv-chart__swatch--cue>svg{display:block}",
    ".rv-chart__readout{position:absolute;left:0;top:0;z-index:2;box-sizing:border-box;max-width:min(18rem,calc(100% - 16px));padding:8px;background:color-mix(in oklch,var(--rv-bg-elev,var(--rv-bg)) 100%,transparent);border:1px solid var(--rv-readout-rule,var(--rv-rule));border-radius:min(var(--rv-radius-s,4px),4px);box-shadow:var(--rv-shadow-float,none);font:12px/1.35 var(--rv-font-ui,system-ui);color:var(--rv-ink);font-variant-numeric:tabular-nums lining-nums;overflow-y:auto;overscroll-behavior:contain;scrollbar-width:thin;pointer-events:none}",
    // One labelled value shares a wrapping row with its heading (S27-900, STY-41 amendment).
    ".rv-chart__readout--compact{display:flex;flex-wrap:wrap;align-items:baseline;gap:6px;padding:6px 8px}",
    ".rv-chart__readout--compact>p{min-width:0}",
    ".rv-chart__readout--compact .rv-chart__readout-line{gap:6px}",
    // Two name and value pairs on one row stand twice as far apart as a name from its value.
    ".rv-chart__readout--compact .rv-chart__readout-line:not(:last-child){margin-right:6px}",
    ".rv-chart__readout--compact .rv-chart__readout-value{margin-left:0}",
    // Between observations the readout glides; reduced motion moves it at once (FEA-CH-42).
    "@media (prefers-reduced-motion:no-preference){.rv-chart__readout{transition:transform 180ms cubic-bezier(.22,1,.36,1),opacity 100ms linear}}",
    ".rv-chart__readout--leaving{opacity:0;pointer-events:none}",
    ".rv-chart__readout[hidden]{display:none}",
    // Line-end names are buttons laid over the names the canvas draws (FEA-CH-27).
    ".rv-chart__names{position:absolute;inset:0;pointer-events:none;z-index:1}",
    // The target is at least 24 px square (WCAG 2.5.8): the padding the runtime sets on each side,
    // in --rv-pt, --rv-pr, --rv-pb and --rv-pl, is hit area only, so the hover tint and the focus
    // ring stay on the drawn name. The sides differ where the target keeps off a dot of its line.
    ".rv-chart__name{position:absolute;pointer-events:auto;box-sizing:border-box;margin:0;padding:var(--rv-pt,0) var(--rv-pr,0) var(--rv-pb,0) var(--rv-pl,0);border:0;border-radius:0;background:none;background-clip:content-box;color:transparent;font-size:1px;cursor:pointer}",
    ".rv-chart__name:hover{background-color:color-mix(in oklch,var(--rv-ink) 7%,transparent)}",
    ".rv-chart__name:focus-visible{outline:none}",
    ".rv-chart__name:focus-visible::after{content:\"\";position:absolute;left:var(--rv-pl,0);right:var(--rv-pr,0);top:var(--rv-pt,0);bottom:var(--rv-pb,0);border-radius:0;outline:2px solid var(--rv-accent);outline-offset:1px}",
    "@media print{.rv-chart__names{display:none}}",
    ".rv-chart__readout p{margin:0;display:flex;gap:.5rem;align-items:baseline}",
    ".rv-chart__readout-head{font-weight:600}",
    // A name (series, field or measure) is set in the soft ink and its value in the full ink, so
    // a row reads value first. The derived value that may follow a single mark's own is set the
    // same way (FEA-CH-40, STY-41 as assumed).
    ".rv-chart__readout-name{color:var(--rv-ink-soft)}",
    // Rows for the series behind an emphasised one step back with them (FEA-CH-52).
    ".rv-chart__readout-line--back{color:var(--rv-ink-soft)}",
    "@media (prefers-reduced-motion:no-preference){.rv-chart__readout-line{transition:color 150ms linear}}",
    ".rv-chart__readout-value{margin-left:auto;font-family:var(--rv-font-mono,ui-monospace)}",
    ".rv-chart__cue{width:9px;height:9px;flex:none;background:var(--rv-swatch)}",
    ".rv-chart__cue--decal{width:auto;height:auto;background:none;align-self:center}",
    ".rv-chart__cue--decal>svg{display:block}",
    ".rv-chart__table{max-height:24rem;overflow:auto;margin-top:.75rem}",
    ".rv-chart__table-status{margin:0 0 .25rem;color:var(--rv-ink-soft);font:var(--rv-size-small,14px)/1.4 var(--rv-font-ui,system-ui)}",
    ".rv-visually-hidden{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);clip-path:inset(50%);white-space:nowrap}",
    ".rv-chart-dialog{--rv-target:32px;width:min(96vw,1400px);max-width:none;max-height:92vh;border:1px solid var(--rv-rule);border-radius:0;background:var(--rv-bg);color:var(--rv-ink);padding:0}",
    ".rv-chart-dialog::backdrop{background:color-mix(in oklch,var(--rv-bg) 94%,transparent)}",
    ".rv-chart-dialog__head{position:sticky;top:0;z-index:4;display:flex;align-items:center;gap:1rem;padding:.75rem 1rem;background:var(--rv-bg);border-bottom:1px solid var(--rv-rule-soft,var(--rv-rule))}",
    // The head holds the chart's title and, under it, its subtitle, set as the inline figure sets
    // them; Close stays beside the pair.
    ".rv-chart-dialog__titles{flex:1;min-width:0}",
    ".rv-chart-dialog__title{margin:0;font:600 calc(var(--rv-size-base,1rem) * .9)/1.3 var(--rv-font-display,inherit)}",
    ".rv-chart-dialog__subtitle{margin:.125rem 0 0;font:var(--rv-size-small,14px)/1.45 var(--rv-font-text,inherit);color:var(--rv-ink-soft)}",
    ".rv-chart.rv-chart--expanded{margin:0;padding:1rem}",
    // On a phone the report's column keeps a 1rem margin each side (report.css, up to 640 px),
    // and the dialog, 96 % of the window wide with a 1 px border, keeps 2 % of the window, so the
    // expanded figure pads its sides only by what that leaves of 1rem: an upright chart is drawn
    // as wide as inline, its edges on the column's (S27-873). The title row keeps its padding,
    // and so does the figure of a turned or short, wide dialog, whose actions sit in the title row
    // placed by that padding (S27-871).
    "@media (max-width:640px){.rv-chart-dialog:not([data-rv-turned]):not([data-rv-short-wide]) .rv-chart.rv-chart--expanded{padding-left:max(0px,calc(1rem - 2vw - 1px));padding-right:max(0px,calc(1rem - 2vw - 1px))}}",
    // While the dialog scrolls, as to an open Table, its actions stay in reach directly under the
    // title row (FEA-CH-45); the runtime sets --rv-head-h to the title row's height. The band is
    // opaque, full width and keeps its spacing as padding, so what scrolls under it is hidden; at
    // rest the buttons sit where they did.
    ".rv-chart--expanded>.rv-chart__actions{position:sticky;top:var(--rv-head-h,0px);z-index:3;align-self:stretch;margin:-.5rem 0 0;padding:.5rem 0 .75rem;background:var(--rv-bg)}",
    // On a touch screen every target in the expanded view is at least 44 px in either orientation,
    // the one size a fingertip meets in the dialog (--rv-target, 32 px with a mouse, sizes the
    // icons of a title row that holds the actions): the actions and Close, the playback buttons,
    // options, select and scrubber, the legend's entries and the overview strip. The title row
    // and the action row give up padding so that the plot keeps its room. The inline figure keeps
    // its sizes.
    "@media (pointer:coarse){.rv-chart-dialog{--rv-target:44px}.rv-chart-dialog .rv-chart__action{min-width:44px;min-height:44px;justify-content:center}.rv-chart-dialog__head{padding-top:.25rem;padding-bottom:.25rem}.rv-chart-dialog .rv-chart--expanded>.rv-chart__actions{padding-bottom:.25rem}",
    ".rv-chart-dialog .rv-chart__control-button,.rv-chart-dialog .rv-chart__option,.rv-chart-dialog .rv-chart__select{min-width:44px;min-height:44px}",
    // A legend entry keeps its drawn size; its target reaches past the swatch and label to 44 px.
    // Its rows and the room above and below are as far apart as that reach, so neither a
    // neighbour nor the action row or the plot takes a tap meant for it.
    ".rv-chart-dialog .rv-chart__legend{row-gap:1.25rem;margin-top:.625rem;margin-bottom:.625rem}.rv-chart-dialog .rv-chart__controls+.rv-chart__legend{margin-top:0}.rv-chart-dialog .rv-chart__legend-entry{position:relative}.rv-chart-dialog .rv-chart__legend-entry::before{content:\"\";position:absolute;inset:min(0px,calc(50% - 22px))}",
    ".rv-chart-dialog .rv-chart__scrubber,.rv-chart-dialog .rv-chart__overview{height:44px}}",
    // On a phone held upright the dialog is laid out in landscape, the screen's height as its
    // width, and turned a quarter about the screen's centre (FEA-CH-68), when the runtime marks
    // it: only where the turned chart is drawn larger than upright.
    "@media ",
    richview_chart::turn_query!(),
    "{.rv-chart-dialog[data-rv-turned]{inset:auto;top:50%;left:50%;margin:0;width:min(96dvh,1400px);max-height:92dvw;transform:translate(-50%,-50%) rotate(90deg)}}",
    // The turned dialog's short side is the plot's height, as is the dialog's in a short, wide
    // window, which the runtime marks: a phone held sideways, a laptop zoomed to 200 % (S27-871,
    // S27-876). Their actions leave their own row and sit in the head row, left of Close, as icons
    // of the pointer's target size; the DOM order stays as upright. They stay there while the
    // dialog scrolls: a row of no height, lifted by the margins into the head row (the title row's
    // height and the figure's padding) and held there as it sticks. The head row keeps a quarter
    // rem above and below whatever the pointer, the actions' offset, and Close keeps the top when
    // the title takes two lines.
    ".rv-chart-dialog[data-rv-turned] .rv-chart--expanded,.rv-chart-dialog[data-rv-short-wide] .rv-chart--expanded{position:static}",
    // A short, wide dialog takes 96 % of the window's height, as of its width: in a window that
    // short every row of the plot counts (S27-875).
    ".rv-chart-dialog[data-rv-short-wide]{max-height:96dvh}",
    ".rv-chart-dialog[data-rv-turned] .rv-chart--expanded>.rv-chart__actions,.rv-chart-dialog[data-rv-short-wide] .rv-chart--expanded>.rv-chart__actions{--rv-actions-top:.25rem;top:var(--rv-actions-top);z-index:4;height:0;min-height:0;margin:calc(var(--rv-actions-top) - var(--rv-head-h,0px) - 1rem) 0 calc(var(--rv-head-h,0px) + 1rem - var(--rv-actions-top));padding:0 calc(1rem + var(--rv-target)) 0 0;background:none;align-items:flex-start;justify-content:flex-end;flex-wrap:nowrap}",
    ".rv-chart-dialog[data-rv-turned] .rv-chart-dialog__head,.rv-chart-dialog[data-rv-short-wide] .rv-chart-dialog__head{padding-top:.25rem;padding-bottom:.25rem}.rv-chart-dialog[data-rv-turned] .rv-chart-dialog__close,.rv-chart-dialog[data-rv-short-wide] .rv-chart-dialog__close{align-self:flex-start}",
    // A set width, not only a least one: engines size the row's content without the buttons'
    // least width, so the last button would overhang Close.
    ".rv-chart-dialog[data-rv-turned] .rv-chart__actions .rv-chart__action,.rv-chart-dialog[data-rv-short-wide] .rv-chart__actions .rv-chart__action,.rv-chart-dialog[data-rv-turned] .rv-chart-dialog__close,.rv-chart-dialog[data-rv-short-wide] .rv-chart-dialog__close{font-size:0;gap:0;padding:0;width:var(--rv-target);height:var(--rv-target);justify-content:center}",
    // There the subtitle costs the plot no height: it stands on the title's line, in the room the
    // title leaves, cut with an ellipsis where it does not all fit, and is left out when fewer
    // than 20 characters of it would show (the dialog still names the whole of it as its
    // description). On a phone held sideways one more line made the dialog scroll for most chart
    // types. Without script: the row wraps, the subtitle asks for at least 20ch and gives its
    // own line no height, so a subtitle sent to a second line is clipped away.
    ".rv-chart-dialog[data-rv-turned] .rv-chart-dialog__titles,.rv-chart-dialog[data-rv-short-wide] .rv-chart-dialog__titles{display:flex;flex-wrap:wrap;align-items:baseline;gap:0 .75rem;overflow:hidden;padding-right:calc(4 * var(--rv-target) + 3 * .375rem + .75rem)}",
    ".rv-chart-dialog[data-rv-turned] .rv-chart-dialog__title,.rv-chart-dialog[data-rv-short-wide] .rv-chart-dialog__title{flex:0 1 auto;min-width:0}.rv-chart-dialog[data-rv-turned] .rv-chart-dialog__subtitle,.rv-chart-dialog[data-rv-short-wide] .rv-chart-dialog__subtitle{flex:1 1 20ch;min-width:20ch;margin:0 0 -1.3em;line-height:1.3;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
    // In a short, wide window an expanded chart narrowed to fit the dialog's height leaves width on
    // its sides. Its legend and note then stand in a column beside the plot, legend first, the
    // pair centred and top-aligned, and the plot takes the height they took (S27-875). The column
    // is as wide as what it holds, up to 16rem with its gap and two fifths of the figure, so on
    // the smallest phone held sideways the plot keeps the larger share; the legend's entries wrap
    // in it. A zoomed chart's overview strip stays under the plot, the column beside both; all
    // else in the figure keeps its full row, and the action row lifted into the title row adds no
    // margin of the next row's to its own, as it did before the figure was a grid.
    ".rv-chart--expanded[data-rv-aside]{display:grid;grid-template-columns:1fr auto fit-content(min(17.5rem,40%)) 1fr;grid-auto-flow:row dense;align-items:start}",
    ".rv-chart--expanded[data-rv-aside]>*{grid-column:1/-1}.rv-chart--expanded[data-rv-aside]>:is(.rv-chart__frame,.rv-chart__overview){grid-column:2}.rv-chart--expanded[data-rv-aside]>.rv-chart__aside{grid-column:3;padding-left:1.5rem}",
    ".rv-chart--expanded[data-rv-aside]>.rv-chart__actions+*{margin-top:0}",
    ".rv-chart-dialog .rv-chart__aside>.rv-chart__legend{margin:0 0 .75rem}.rv-chart-dialog .rv-chart__aside>.rv-chart__method{margin:0}",
    // Each icon shows its name on hover and on keyboard focus: the label under the button in the
    // readout's surface, faded in with the readout's motion, and still the button's accessible
    // name (S27-876). A row at the right aligns each name to its button's right edge, and the
    // upright dialog's row, at the left, to its left edge, so no name leaves the figure or the
    // dialog. A tap shows none, since a touch screen keeps hover after the tap.
    ".rv-chart__action>span{",
    action_name!(),
    "}",
    ".rv-chart-dialog:not([data-rv-turned]):not([data-rv-short-wide]) .rv-chart--expanded>.rv-chart__actions .rv-chart__action>span{left:0;right:auto}",
    "@media (hover:hover){.rv-chart__action:hover>span{opacity:1}}",
    ".rv-chart__action:focus-visible>span{opacity:1}",
    "@media (prefers-reduced-motion:no-preference){.rv-chart__action>span{transition:opacity 120ms cubic-bezier(.22,1,.36,1)}}",
    // Each control row such a dialog wraps to takes its targets' height from the plot's, so the
    // scrubber starts at 5.5rem and takes what its row leaves: the playback and the options share
    // a row where they fit side by side (S27-870).
    ".rv-chart-dialog[data-rv-turned] .rv-chart__playback,.rv-chart-dialog[data-rv-short-wide] .rv-chart__playback{flex:1 1 auto}.rv-chart-dialog[data-rv-turned] .rv-chart__scrubber,.rv-chart-dialog[data-rv-short-wide] .rv-chart__scrubber{width:5.5rem;flex:1 1 5.5rem}",
    ".rv-scroll-locked{overflow:hidden}",
    // Reader controls (FEA-BR-06, FEA-BR-34): one wrapping row above the plot in the UI face,
    // buttons with the active option in the accent, a restyled native select and a scrubber in
    // the rule and accent colours; the frame value keeps a fixed width.
    ".rv-chart__controls{display:flex;flex-wrap:wrap;align-items:center;gap:.5rem 1.25rem;margin:0 0 .75rem}",
    ".rv-chart__controls[data-rv-pending]{visibility:hidden}",
    ".rv-chart__control{display:inline-flex;flex-wrap:wrap;align-items:center;gap:.375rem;margin:0}",
    ".rv-chart__control-label{margin-right:.25rem;font:var(--rv-size-small,14px)/1.2 var(--rv-font-ui,system-ui);color:var(--rv-ink-soft)}",
    ".rv-chart__option,.rv-chart__control-button,.rv-chart__select{font:var(--rv-size-small,14px)/1.2 var(--rv-font-ui,system-ui);color:var(--rv-ink);background:transparent;border:1px solid var(--rv-rule);border-radius:0;min-height:2rem;cursor:pointer}",
    ".rv-chart__option{padding:.3rem .65rem}",
    ".rv-chart__option:hover,.rv-chart__control-button:hover,.rv-chart__select:hover{border-color:var(--rv-ink-soft)}",
    ".rv-chart__option[aria-pressed=true]{border-color:var(--rv-accent);color:var(--rv-accent);background:color-mix(in oklch,var(--rv-accent) 10%,transparent)}",
    ".rv-chart__control-button{display:inline-flex;align-items:center;justify-content:center;width:2rem;padding:0}",
    ".rv-chart__control-button svg{width:14px;height:14px}",
    ".rv-chart__control-button .rv-chart__pause,.rv-chart__control-button[aria-pressed=true] .rv-chart__play-shape{display:none}",
    ".rv-chart__control-button[aria-pressed=true] .rv-chart__pause{display:inline}",
    ".rv-chart__select{-webkit-appearance:none;appearance:none;padding:.3rem 1.75rem .3rem .65rem;background-image:linear-gradient(45deg,transparent 50%,currentColor 50%),linear-gradient(135deg,currentColor 50%,transparent 50%);background-position:calc(100% - 13px) 55%,calc(100% - 8px) 55%;background-size:5px 5px;background-repeat:no-repeat}",
    ".rv-chart__scrubber{-webkit-appearance:none;appearance:none;width:min(14rem,40vw);height:2rem;margin:0;background:transparent;cursor:pointer}",
    ".rv-chart__scrubber::-webkit-slider-runnable-track{height:2px;background:var(--rv-rule)}",
    ".rv-chart__scrubber::-moz-range-track{height:2px;background:var(--rv-rule)}",
    ".rv-chart__scrubber::-webkit-slider-thumb{-webkit-appearance:none;width:14px;height:14px;margin-top:-6px;border-radius:50%;background:var(--rv-accent);border:2px solid var(--rv-bg)}",
    ".rv-chart__scrubber::-moz-range-thumb{width:14px;height:14px;border-radius:50%;background:var(--rv-accent);border:2px solid var(--rv-bg)}",
    ".rv-chart__frame-value{display:inline-block;font:var(--rv-size-small,14px)/1.2 var(--rv-font-mono,ui-monospace);font-variant-numeric:tabular-nums;color:var(--rv-ink)}",
    ".rv-chart__option:focus-visible,.rv-chart__control-button:focus-visible,.rv-chart__select:focus-visible,.rv-chart__scrubber:focus-visible{outline:2px solid var(--rv-accent);outline-offset:2px}",
    // The state in words: printed and in PDF, where the controls are gone (FEA-BR-02).
    ".rv-chart__state{display:none;margin:0 0 .5rem;font:var(--rv-size-small,14px)/1.4 var(--rv-font-ui,system-ui);color:var(--rv-ink-soft)}",
    ".rv-chart--print .rv-chart__state{display:block}",
    "@media print{.rv-chart__controls{display:none}.rv-chart__state{display:block}}",
    "@media print{.rv-chart__frame::before{content:none!important}.rv-chart__actions,.rv-chart__readout,.rv-chart__notice{display:none}.rv-chart__legend-entry{text-decoration:none;opacity:1}.rv-chart{break-inside:avoid}}",
    // A browser lays the page out for paper after `beforeprint`, with no script in between, so a
    // chart drawn at the screen's width meets the printed column as it is: it scales down to fit,
    // in proportion, and its frame takes the drawing's height (FEA-CH-67, FEA-RP-34).
    "@media print{.rv-chart__canvas{max-width:100%!important;height:auto!important}.rv-chart__frame{height:auto!important}}",
);

/// The style's `motion` section as the profile the chart engine plays entrances with.
/// A motion number, bare or quoted: the style grammar accepts both.
fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

fn motion_profile(style: &ResolvedStyle) -> richview_chart::motion::Profile {
    let motion = &style.document["motion"];
    let defaults = richview_chart::motion::Profile::default();
    richview_chart::motion::Profile {
        character: motion["character"]
            .as_str()
            .map_or(defaults.character, str::to_owned),
        duration_ms: number(&motion["duration"]).unwrap_or(defaults.duration_ms),
        easing: richview_chart::motion::easing(motion["easing"].as_str().unwrap_or("standard")),
        stagger_ms: number(&motion["stagger"]).unwrap_or(defaults.stagger_ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(v: Value) -> ChartBlock {
        ChartBlock::from_value(&v).expect("block")
    }

    fn context(theme: &ChartTheme, pdf: Option<[f64; 2]>) -> Context<'_> {
        Context {
            theme,
            print: None,
            lang: "en",
            colors: &[],
            widths: [358.0, 832.0],
            pdf,
            stacked: None,
        }
    }

    #[test]
    fn payload_carries_referenced_fields_at_readout_precision() {
        let b = block(json!({"type": "line", "title": "T", "encoding": {"x": "t", "y": "v"}}));
        let rows = vec![
            json!({"t": 1.0001, "v": 1234.567, "extra": 1}),
            json!({"t": 1.0002, "v": 0.000123456}),
        ];
        let (columns, out) = project(&b, &rows, &[]);
        assert_eq!(columns, ["t", "v"], "only referenced fields travel");
        assert_eq!(
            out[0][0],
            json!(1.0001),
            "x keeps the digits that tell positions apart"
        );
        assert_eq!(out[1][0], json!(1.0002));
        assert_eq!(out[0][1], json!(1235), "whole thousands keep four digits");
        assert_eq!(out[1][1], json!(0.000123));
    }

    #[test]
    fn decimal_text_in_value_channels_is_rounded_but_category_text_is_not() {
        let b = block(json!({"type": "bar", "title": "T", "encoding": {"x": "zip", "y": "v"}}));
        let rows = vec![json!({"zip": "02134", "v": "1.234567890123456789"})];
        let (_, out) = project(&b, &rows, &[]);
        assert_eq!(out[0][0], json!("02134"), "identifiers stay text");
        assert_eq!(
            out[0][1],
            json!(1.23),
            "FEA-CH-37: no digit beyond the author's precision"
        );
    }

    #[test]
    fn source_column_order_wins_when_recorded() {
        let b = block(
            json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v", "color": "s"}}),
        );
        let (columns, _) = project(&b, &[], &["s".into(), "v".into(), "c".into()]);
        assert_eq!(columns, ["s", "v", "c"]);
    }

    #[test]
    fn a_waiting_chart_shows_the_loading_state_and_the_notice_only_without_scripts() {
        // The loader is the frame's own pseudo-element: no element, so readers without CSS or
        // scripts meet only the notice (ARC-05).
        let loader = ".rv-chart:not(.rv-chart--print)>.rv-chart__frame:not([data-rv-painted],[data-rv-notice])::before{content:";
        let scripting = FIGURE_CSS.find("@media (scripting:enabled){").unwrap();
        let at = FIGURE_CSS.find(loader).unwrap();
        assert!(at > scripting, "the loader shows only where scripts run");
        assert!(FIGURE_CSS.contains(
            ".rv-chart__frame:not([data-rv-notice])>.rv-chart__notice{visibility:hidden}"
        ));
        assert!(
            FIGURE_CSS.contains("@media print{.rv-chart__frame::before{content:none!important}")
        );
        assert!(
            FIGURE_CSS.contains("@media (prefers-reduced-motion:reduce){.rv-chart__frame::before")
        );
    }

    #[test]
    fn figure_reserves_its_box_and_embeds_rows_once() {
        let value = json!({"type": "bar", "title": "Sales <rose>", "encoding": {"x": "q", "y": "v"},
            "data": {"inline": [{"q": "Q1", "v": 1}, {"q": "Q2", "v": 2}]}});
        let rows = value["data"]["inline"].as_array().unwrap().clone();
        let theme = ChartTheme::default();
        let f = chart(&value, &rows, &[], "rv-chart-0", 3, &context(&theme, None));
        assert!(f.html.contains("--rv-ratio:"), "{}", f.html);
        assert!(f.html.contains(NOTICE));
        assert!(f.html.contains("Sales &lt;rose&gt;"));
        assert!(
            !f.html.contains("<rose>"),
            "title escaped in caption and payload"
        );
        let payload = f.payload.unwrap();
        assert!(payload["block"].get("data").is_none());
        // Native integer rows hold unit steps; the shown values keep the author's numbers.
        assert_eq!(payload["rows"], json!([["Q1", 0], ["Q2", 1]]));
        assert_eq!(payload["quant"]["v"]["resolution"].as_f64(), Some(1.0));
        assert_eq!(
            payload["shown"].as_object().unwrap().values().next(),
            Some(&json!([1, 2]))
        );
        assert_eq!(payload["file"], "sales-rose");
    }

    #[test]
    fn pdf_figures_draw_vector_geometry_without_a_payload() {
        let value = json!({"type": "line", "title": "T", "encoding": {"x": "x", "y": "y"}});
        let rows = vec![json!({"x": 1, "y": 2}), json!({"x": 2, "y": 3})];
        let theme = ChartTheme::default();
        let f = chart(
            &value,
            &rows,
            &[],
            "rv-chart-0",
            1,
            &context(&theme, Some([600.0, 900.0])),
        );
        assert!(f.payload.is_none());
        assert!(f.html.contains("<svg"));
        assert!(!f.html.contains(NOTICE));
    }

    #[test]
    fn a_tall_chart_prints_narrower_within_sixty_percent_of_the_page() {
        let svg_size = |html: &str| -> [f64; 2] {
            let svg = &html[html.find("<svg").unwrap()..];
            let attr = |name: &str| {
                let at = svg.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
                svg[at..at + svg[at..].find('"').unwrap()].parse().unwrap()
            };
            [attr("width"), attr("height")]
        };
        let rows = vec![json!({"x": "A", "y": 2}), json!({"x": "B", "y": 3})];
        let theme = ChartTheme::default();
        let print = |ratio: &str| {
            let value = json!({"type": "bar", "title": "T", "encoding": {"x": "x", "y": "y"}, "style": {"ratio": ratio}});
            chart(
                &value,
                &rows,
                &[],
                "rv-chart-0",
                1,
                &context(&theme, Some([600.0, 900.0])),
            )
            .html
        };
        let [width, height] = svg_size(&print("1:2"));
        assert!(height <= 540.0, "{width} x {height}");
        assert!(width < 600.0, "{width} x {height}");
        // A chart that already fits keeps the full column.
        let [width, height] = svg_size(&print("16:9"));
        assert_eq!(width, 600.0, "{width} x {height}");
    }
}
