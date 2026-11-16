//! The scrollytelling element (FEA-RP-48): one image that stays in view beside ordered narrative
//! steps, each naming the state the figure takes when it scrolls into view. The static HTML is
//! complete without scripts: the figure shows its first state and the steps read in order. The
//! runtime (`richview-runtime`, `web/scrolly.rs`) moves the figure as steps scroll in; on paper
//! the figure prints once and every step with its own state prints a still of it.
//!
//! assumed: the fence tag is `scrolly`, its figure is an `image` mapping (a chart figure waits for
//! the chart state model, TYP-77), and a step's state is `view` (`x`, `y` as fractions of the
//! image and a `zoom` of at least 1) plus an optional `highlight` box (`x`, `y`, `w`, `h`).

use crate::markdown::{Context, markdown_html};
use crate::{Diagnostic, RenderOptions, escape};
use serde_json::Value;

/// The problems in a `scrolly` mapping, as `(key, message)`.
pub(crate) fn validate(map: &serde_json::Map<String, Value>) -> Vec<(String, String)> {
    let mut issues = vec![];
    if map.get("title").is_some_and(|v| !v.is_string()) {
        issues.push(("title".into(), "expected a string".into()));
    }
    match (map.get("image"), map.get("chart")) {
        (Some(Value::Object(image)), None) if image.get("src").is_some_and(Value::is_string) => {}
        (None, Some(Value::Object(_))) => {}
        (Some(_), Some(_)) => issues.push((
            "chart".into(),
            "give the figure as `image` or `chart`, not both".into(),
        )),
        (None, Some(_)) => issues.push(("chart".into(), "expected a chart mapping".into())),
        _ => issues.push((
            "image".into(),
            "expected an image mapping with a src string, or a `chart` mapping".into(),
        )),
    }
    match map.get("steps") {
        Some(Value::Array(steps)) if !steps.is_empty() => {
            for (i, step) in steps.iter().enumerate() {
                let Some(step) = step.as_object() else {
                    issues.push((format!("steps[{i}]"), "expected a mapping".into()));
                    continue;
                };
                if !step.get("text").is_some_and(Value::is_string) {
                    issues.push((
                        format!("steps[{i}].text"),
                        "expected Markdown string".into(),
                    ));
                }
                if step.get("title").is_some_and(|v| !v.is_string()) {
                    issues.push((format!("steps[{i}].title"), "expected a string".into()));
                }
                if step.get("state").is_some_and(|v| !v.is_object()) {
                    issues.push((
                        format!("steps[{i}].state"),
                        "expected a mapping of control id to option value".into(),
                    ));
                }
                if let Some(view) = step.get("view") {
                    if view.as_object().is_none() {
                        issues.push((format!("steps[{i}].view"), "expected a mapping".into()));
                    } else if view_of(step).is_none() {
                        issues.push((
                            format!("steps[{i}].view"),
                            "expected zoom of at least 1 with x and y between 0 and 1".into(),
                        ));
                    }
                }
                if let Some(mark) = step.get("highlight")
                    && mark_of(mark).is_none()
                {
                    issues.push((
                        format!("steps[{i}].highlight"),
                        "expected x, y, w and h as fractions of the image".into(),
                    ));
                }
            }
        }
        _ => issues.push(("steps".into(), "expected a non-empty list of steps".into())),
    }
    issues
}

/// A step's view as `(x, y, zoom)`, the top-left corner kept inside the image.
fn view_of(step: &serde_json::Map<String, Value>) -> Option<(f64, f64, f64)> {
    let Some(view) = step.get("view") else {
        return Some((0.0, 0.0, 1.0));
    };
    let number = |key: &str, default: f64| match view.get(key) {
        None => Some(default),
        Some(v) => v.as_f64().filter(|n| n.is_finite()),
    };
    let zoom = number("zoom", 1.0)?;
    let (x, y) = (number("x", 0.0)?, number("y", 0.0)?);
    if !(1.0..=20.0).contains(&zoom) || !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return None;
    }
    let room = 1.0 - 1.0 / zoom;
    Some((x.min(room), y.min(room), zoom))
}

fn mark_of(mark: &Value) -> Option<(f64, f64, f64, f64)> {
    let get = |key: &str| mark.get(key)?.as_f64().filter(|n| (0.0..=1.0).contains(n));
    let (x, y, w, h) = (get("x")?, get("y")?, get("w")?, get("h")?);
    (w > 0.0 && h > 0.0 && x + w <= 1.0 + 1e-9 && y + h <= 1.0 + 1e-9).then_some((x, y, w, h))
}

fn num(n: f64) -> String {
    let text = format!("{:.4}", n);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A step's figure state: custom properties for the view and the highlight box in the view's own
/// percentages, as one `style` value, and the same numbers as data attributes for the runtime.
struct State {
    style: String,
    view: String,
    mark: String,
}

fn state(step: &serde_json::Map<String, Value>) -> State {
    let (x, y, zoom) = view_of(step).unwrap_or((0.0, 0.0, 1.0));
    let mark = step
        .get("highlight")
        .and_then(mark_of)
        .map(|(mx, my, mw, mh)| {
            [
                (mx - x) * zoom * 100.0,
                (my - y) * zoom * 100.0,
                mw * zoom * 100.0,
                mh * zoom * 100.0,
            ]
        });
    let pct = |n: f64| format!("{}%", num(n));
    let (mark_style, mark_attr) = match mark {
        Some([l, t, w, h]) => (
            format!(
                "--rv-ml:{};--rv-mt:{};--rv-mw:{};--rv-mh:{};--rv-mo:1",
                pct(l),
                pct(t),
                pct(w),
                pct(h)
            ),
            format!("{} {} {} {}", num(l), num(t), num(w), num(h)),
        ),
        None => ("--rv-mo:0".to_owned(), String::new()),
    };
    State {
        style: format!(
            "--rv-vx:{};--rv-vy:{};--rv-vz:{};{mark_style}",
            num(x),
            num(y),
            num(zoom)
        ),
        view: format!("{} {} {}", num(x), num(y), num(zoom)),
        mark: mark_attr,
    }
}

/// The `<img ...>` tag of rendered media, when it rendered one.
fn img_tag(figure: &str) -> Option<&str> {
    let start = figure.find("<img")?;
    let end = start + figure[start..].find('>')? + 1;
    Some(&figure[start..end])
}

/// A chart figure for the stage: its rendered figure and, per step, the control choices it sets
/// (`control:option` pairs for the runtime) and its print still.
pub(crate) struct ChartStage {
    pub html: String,
    pub choices: Vec<String>,
    pub stills: Vec<String>,
}

pub(crate) fn render(
    value: &Value,
    options: &RenderOptions,
    lang: &str,
    placeholder: &str,
    diagnostics: &mut Vec<Diagnostic>,
    line: usize,
    chart: Option<ChartStage>,
) -> String {
    let empty = serde_json::Map::new();
    let image = value.get("image").unwrap_or(&Value::Null);
    let figure = match &chart {
        Some(chart) => chart.html.clone(),
        None => crate::blocks::render_media("image", image, options, lang, diagnostics, line),
    };
    let img = img_tag(&figure)
        .map(str::to_owned)
        .filter(|_| chart.is_none());
    let steps = value
        .get("steps")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let first = steps
        .first()
        .and_then(Value::as_object)
        .map(state)
        .unwrap_or_else(|| state(&empty));
    // The frame clips the image to the current view and carries the highlight box.
    let stage = match &img {
        Some(img) => {
            let framed = format!(
                "<div class=\"rv-scrolly__frame\">{img}<span class=\"rv-scrolly__mark\" aria-hidden=\"true\"></span></div>"
            );
            figure.replacen(img.as_str(), &framed, 1)
        }
        None => figure.clone(),
    };
    let mut list = String::new();
    for (i, step) in steps.iter().enumerate() {
        let Some(step) = step.as_object() else {
            continue;
        };
        let s = state(step);
        let title = step
            .get("title")
            .and_then(Value::as_str)
            .map(|t| {
                format!(
                    "<p class=\"rv-scrolly__step-title\"><b>{}</b></p>",
                    escape(t)
                )
            })
            .unwrap_or_default();
        let path = format!("scrolly.steps[{i}].text");
        let text = markdown_html(
            step.get("text").and_then(Value::as_str).unwrap_or(""),
            options,
            &Context {
                heading_ids: &[],
                first_line: Some(line),
                columns: false,
                path: &path,
                scope: &format!("s{line}n{i}-"),
                placeholder,
                lang,
            },
            diagnostics,
        );
        // A step with a state of its own prints a still of it; the others read under the figure.
        let own = step.contains_key("view") || step.contains_key("highlight");
        let chart_still = chart
            .as_ref()
            .and_then(|c| c.stills.get(i))
            .filter(|still| !still.is_empty());
        let still = match (&img, own) {
            _ if chart_still.is_some() => format!(
                "<div class=\"rv-scrolly__still rv-print-only\">{}</div>",
                chart_still.map(String::as_str).unwrap_or_default()
            ),
            (Some(img), true) => format!(
                "<div class=\"rv-scrolly__still rv-print-only\" style=\"{}\"><div class=\"rv-scrolly__frame\">{img}<span class=\"rv-scrolly__mark\" aria-hidden=\"true\"></span></div></div>",
                escape(&s.style)
            ),
            _ => String::new(),
        };
        let current = if i == 0 { " aria-current=\"step\"" } else { "" };
        let choices = chart
            .as_ref()
            .and_then(|c| c.choices.get(i))
            .filter(|c| !c.is_empty())
            .map(|c| format!(" data-rv-controls=\"{}\"", escape(c)))
            .unwrap_or_default();
        list.push_str(&format!(
            "<li class=\"rv-scrolly__step\" data-rv-step data-rv-view=\"{}\" data-rv-mark=\"{}\"{choices}{current}>{still}{title}{text}</li>",
            escape(&s.view),
            escape(&s.mark)
        ));
    }
    let label = value
        .get("title")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
        .unwrap_or("Story");
    format!(
        "<section class=\"rv-scrolly\" data-rv-scrolly aria-label=\"{}\" style=\"{}\"><div class=\"rv-scrolly__stage\">{stage}</div><ol class=\"rv-scrolly__steps\">{list}</ol></section>",
        escape(label),
        escape(&first.style)
    )
}
