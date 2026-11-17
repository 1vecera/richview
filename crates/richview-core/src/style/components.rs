//! The CSS a style's `components` section generates: text transforms, rules and bars, the
//! metric and callout variants, drawn pencil marks and the structure of a chart's figure
//! (outline or shading, where its title and description sit; STY-01, STY-33, STY-34, STY-67).

use super::sink_refuses;
use crate::text;
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn css(components: Option<&Value>, props: &BTreeMap<String, String>) -> String {
    let Some(c) = components else {
        return String::new();
    };
    let mut css = String::new();
    if c.pointer("/header/border-top")
        .and_then(Value::as_str)
        .is_some_and(|v| v != "0px")
    {
        css.push_str(".rv-report__header{padding-top:calc(var(--rv-line) * .75);padding-bottom:var(--rv-line)}\n");
    }
    // The base metric is a value under one hairline rule (`rule`). `bracket` closes it with a
    // second rule; `underline` draws one pencil stroke under the value (see `drawn_marks_css`);
    // `knockout` is a true knockout: a square block filled with the accent and bordered in 2 px
    // ink, value, label and delta in white. It and code panels are the only filled blocks, and
    // the delta's sign and words carry the trend, not its colour (STY-33).
    match c.pointer("/metric/style").and_then(Value::as_str) {
        Some("bracket") => css.push_str(".rv-metric{border-bottom:1px solid var(--rv-rule);padding-bottom:calc(var(--rv-line) * .5 * var(--rv-density))}\n"),
        Some("knockout") => css.push_str(".rv-metric{background:var(--rv-accent);border:2px solid var(--rv-ink);padding:calc(var(--rv-line) * .75 * var(--rv-density)) 1.25rem;color:oklch(1 0 0)}.rv-metric__value,.rv-metric__label,.rv-metric__delta,.rv-metric__delta--up,.rv-metric__delta--down{color:oklch(1 0 0)}\n"),
        _ => {}
    }
    const TITLE: &str = ".rv-report__title";
    // The expanded view's title is the chart's title, in a dialog outside the body. Each takes
    // its face and weight from the figure's later `font` rule of the same specificity, and its
    // case and tracking from here.
    const HEADINGS: &str = ".rv-report__body h1,.rv-report__body h2,.rv-report__body h3,.rv-callout__title,.rv-chart .rv-chart__title,.rv-chart-dialog__title";
    // A chart's expanded view is a dialog outside the body; its Table takes the same head.
    const TABLE_HEAD: &str = ".rv-report__body th,.rv-chart-dialog th";
    const SOURCE_LABEL: &str = ".rv-report__body .rv-source__label";
    for (path, selector, property, suffix) in [
        (
            "/callout/border-width",
            ".rv-callout",
            "border-top-width",
            "",
        ),
        (
            "/header/border-top",
            ".rv-report__header",
            "border-top",
            " solid var(--rv-ink)",
        ),
        (
            "/header/border-bottom",
            ".rv-report__header",
            "border-bottom",
            " solid var(--rv-ink)",
        ),
        (
            "/title/transform",
            ".rv-report__title",
            "text-transform",
            "",
        ),
        ("/title/weight", ".rv-report__title", "font-weight", ""),
        (
            "/title/letter-spacing",
            ".rv-report__title",
            "letter-spacing",
            "",
        ),
        ("/heading/transform", HEADINGS, "text-transform", ""),
        ("/heading/weight", HEADINGS, "font-weight", ""),
        ("/heading/letter-spacing", HEADINGS, "letter-spacing", ""),
        ("/table-head/transform", TABLE_HEAD, "text-transform", ""),
        ("/table-head/weight", TABLE_HEAD, "font-weight", ""),
        (
            "/table-head/letter-spacing",
            TABLE_HEAD,
            "letter-spacing",
            "",
        ),
        (
            "/source-label/transform",
            SOURCE_LABEL,
            "text-transform",
            "",
        ),
        ("/source-label/weight", SOURCE_LABEL, "font-weight", ""),
        (
            "/source-label/letter-spacing",
            SOURCE_LABEL,
            "letter-spacing",
            "",
        ),
    ] {
        if let Some(v) = c.pointer(path).filter(|v| !sink_refuses(&text(v))) {
            css.push_str(&format!("{selector}{{{property}:{}{suffix}}}\n", text(v)));
        }
    }
    for (component, selector) in [
        ("title", TITLE),
        ("heading", HEADINGS),
        ("table-head", TABLE_HEAD),
        ("source-label", SOURCE_LABEL),
    ] {
        if let Some(font) = c
            .get(component)
            .and_then(|leaves| leaves.get("font"))
            .and_then(Value::as_str)
        {
            css.push_str(&format!(
                "{selector}{{font-family:var(--rv-font-{font})}}\n"
            ));
        }
    }
    css.push_str(&figure_css(c));
    css.push_str(&drawn_marks_css(c, props));
    css
}

/// Drawn page marks: header, section, footer and table-head rules, the callout bracket and the
/// stat underline as seeded pencil strokes (`richview_chart::pencil`). Each stroke is an image in
/// its token's colour, sized to its element, and keeps printing with the page. Gridlines, axes
/// and data marks are never drawn this way.
fn drawn_marks_css(c: &Value, props: &BTreeMap<String, String>) -> String {
    use richview_chart::pencil;
    // A stroke is an image, so it carries its colour: the style's token, or the default ink and
    // accent of `report.css` when a style draws marks without defining them.
    let token = |name: &str| props.get(&format!("--rv-{name}")).map(String::as_str);
    let ink = token("ink").unwrap_or("oklch(0.22 0.015 40)");
    let faint = token("ink-faint").or(token("ink-soft")).unwrap_or(ink);
    let accent = token("accent").unwrap_or("oklch(0.55 0.18 277)");
    const PRINT: &str = "-webkit-print-color-adjust:exact;print-color-adjust:exact";
    let mut css = String::new();
    if c.pointer("/rules/style").and_then(Value::as_str) == Some("pencil") {
        let rule = |id: &str, weight: f64, fill: &str| {
            stroke_image(
                &pencil::rule(id, 640.0, 8.0, weight),
                (0.0, 0.0, 640.0, 8.0),
                fill,
            )
        };
        css.push_str(&format!(
            ".rv-report__header{{border-bottom:0;background:{} left bottom/100% 8px no-repeat;{PRINT}}}\n",
            rule("rule.header", 1.7, ink)
        ));
        css.push_str(&format!(
            ".rv-report__footer{{border-top:0;background:{} left top/100% 8px no-repeat;{PRINT}}}\n",
            rule("rule.footer", 1.3, faint)
        ));
        // Section rules vary between three strokes; the first section follows the header rule.
        css.push_str(".rv-report__body>h2{padding-top:calc(var(--rv-line) * .7)}\n");
        for (index, selector) in ["3n+1", "3n+2", "3n"].iter().enumerate() {
            css.push_str(&format!(
                ".rv-report__body>h2:nth-of-type({selector}){{background:{} left top/100% 8px no-repeat;{PRINT}}}\n",
                rule(&format!("rule.section.{}", index + 1), 1.3, faint)
            ));
        }
        css.push_str(".rv-report__body>h2:first-child{padding-top:0;background:none}\n");
        css.push_str(&format!(
            ".rv-report__body thead th,.rv-chart-dialog thead th{{border-bottom:0}}.rv-report__body thead,.rv-chart-dialog thead{{background:{} left bottom/100% 8px no-repeat;{PRINT}}}\n",
            rule("rule.table-head", 1.5, ink)
        ));
    }
    if c.pointer("/rules/style").and_then(Value::as_str) == Some("pencil") {
        // Struck text is crossed by a pencil stroke instead of a ruled line (STY-35).
        let strike = stroke_image(
            &pencil::strike("text.strike", 120.0, 10.0, 1.6),
            (0.0, 0.0, 120.0, 10.0),
            ink,
        );
        css.push_str(&format!(
            ".rv-report__body :is(del,s){{text-decoration:none;background:{strike} left center/100% .55em no-repeat;{PRINT}}}\n"
        ));
    }
    if c.pointer("/callout/style").and_then(Value::as_str) == Some("bracket") {
        // One bracket stroke in three slices: fixed hooks at the top and bottom and the stem
        // between them, which stretches with the callout.
        let d = pencil::bracket("callout.bracket", 12.0, 120.0, 1.7);
        let top = stroke_image(&d, (0.0, 0.0, 12.0, 12.0), ink);
        let stem = stroke_image(&d, (0.0, 11.0, 12.0, 98.0), ink);
        let bottom = stroke_image(&d, (0.0, 108.0, 12.0, 12.0), ink);
        css.push_str(&format!(
            ".rv-callout{{border-top:0;padding-left:calc(12px + .9em);background:{top} left top/12px 12px no-repeat,{stem} left 0 top 11px/12px calc(100% - 22px) no-repeat,{bottom} left bottom/12px 12px no-repeat;{PRINT}}}\n"
        ));
    }
    if c.pointer("/metric/style").and_then(Value::as_str) == Some("underline") {
        let stroke = stroke_image(
            &pencil::underline("big.underline", 180.0, 10.0, 3.2),
            (0.0, 0.0, 180.0, 10.0),
            accent,
        );
        // The value keeps its own line; the stroke runs a little past both ends of the figure.
        css.push_str(&format!(
            ".rv-metric{{border-top:0;padding-top:0}}.rv-metric::after{{content:\"\";flex:0 0 100%;order:1}}.rv-metric__label,.rv-metric__delta{{order:2}}.rv-metric__value{{flex:0 0 auto;position:relative;margin-bottom:.12em}}.rv-metric__value::after{{content:\"\";position:absolute;left:-.08em;right:-.14em;bottom:-.1em;height:.3em;background:{stroke} 0 0/100% 100% no-repeat;{PRINT}}}\n"
        ));
    }
    css
}

/// An SVG path as a CSS image. The `view` box stretches to whatever size the background is
/// given, so a stroke drawn once follows its element's width (or height).
fn stroke_image(d: &str, view: (f64, f64, f64, f64), fill: &str) -> String {
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='{} {} {} {}' preserveAspectRatio='none'><path fill='{fill}' d='{d}'/></svg>",
        view.0, view.1, view.2, view.3
    );
    let mut url = String::with_capacity(svg.len() + 40);
    url.push_str("url(\"data:image/svg+xml,");
    for ch in svg.chars() {
        match ch {
            '%' => url.push_str("%25"),
            '#' => url.push_str("%23"),
            '<' => url.push_str("%3C"),
            '>' => url.push_str("%3E"),
            '"' => url.push_str("%22"),
            ch => url.push(ch),
        }
    }
    url.push_str("\")");
    url
}

/// The width a chart's frame takes from the figure on each side pair, in CSS pixels: the shaded
/// and outlined frames pad the figure by 1.25rem a side (and the outline adds its 2px border), so
/// the plot is narrower than the column. Core lays the chart out at the width it will have.
pub(crate) fn frame_inset_px(document: &Value) -> f64 {
    match document
        .pointer("/components/figure/frame")
        .and_then(Value::as_str)
    {
        Some("shaded") => 40.0,
        Some("outline") => 44.0,
        _ => 0.0,
    }
}

/// The structure of a chart's figure: its frame, and where its title and description sit
/// (STY-01, STY-39). The default, a title over a description with the actions beside it, needs
/// no rules; any other placement lays the figure out as one column whose parts are ordered, with
/// the head's rows opened so each part moves on its own.
fn figure_css(c: &Value) -> String {
    let get = |name: &str| {
        c.pointer(&format!("/figure/{name}"))
            .and_then(Value::as_str)
    };
    let mut css = String::new();
    match get("frame") {
        Some("shaded") => css.push_str(".rv-chart{background:var(--rv-bg-chart);padding:1rem 1.25rem;border-radius:var(--rv-radius-m)}\n"),
        Some("outline") => css.push_str(".rv-chart{border:2px solid var(--rv-ink);padding:1rem 1.25rem}\n"),
        Some("rule-above") => css.push_str(".rv-chart{border-top:2px solid var(--rv-ink);padding-top:.75rem}\n"),
        _ => {}
    }
    // The expanded view is a dialog, not a page block: it keeps no frame of its own.
    if get("frame").is_some_and(|frame| frame != "none") {
        css.push_str(".rv-chart.rv-chart--expanded{border:0;background:none;border-radius:0}\n");
    }
    let (title, description) = (get("title"), get("description"));
    if title.unwrap_or("above") != "above" || description.unwrap_or("above") != "above" {
        let order = |position: Option<&str>, above: u8, below: u8| {
            if position.unwrap_or("above") == "above" {
                above
            } else {
                below
            }
        };
        // The title row keeps its actions at its right end (FEA-CH-43); a title set below the
        // chart leaves them at the top right of the figure, where the expanded view has its own.
        css.push_str(&format!(
            ".rv-chart{{display:flex;flex-direction:column}}.rv-chart>*{{order:5}}.rv-chart .rv-chart__head{{display:contents}}.rv-chart .rv-chart__titlerow{{order:{}}}.rv-chart .rv-chart__subtitle{{order:{};flex:none}}.rv-chart--expanded>.rv-chart__actions{{order:3;align-self:flex-end;margin-left:0;justify-content:flex-end}}.rv-chart .rv-chart__method{{order:9}}\n",
            order(title, 1, 7),
            order(description, 2, 8),
        ));
        if title.unwrap_or("above") != "above" {
            css.push_str(".rv-chart .rv-chart__titlerow{display:contents}.rv-chart .rv-chart__titlerow>.rv-chart__title{order:7;flex:none}.rv-chart .rv-chart__titlerow>.rv-chart__actions{order:3;align-self:flex-end;margin:0}\n");
            // On a phone too the icons keep their own line over the chart, unlike every other
            // style's (Daniel, 17 Nov: "no empty line for icons"). That line is the only place a
            // phone's readout clears its mark on chord and venn, whose plot fills the width. The
            // phone row returns when chord and venn are judged by painted mark (Readout fit,
            // train 4).
        }
    }
    if description == Some("below-boxed") {
        css.push_str(".rv-chart .rv-chart__subtitle{border:1px solid var(--rv-rule);padding:.5rem .75rem;margin-top:.5rem}\n");
    }
    css
}
