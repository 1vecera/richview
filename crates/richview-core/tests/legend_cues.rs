//! The report's legend before the runtime starts draws each series' hatch where the style tells
//! series apart by it (FEA-CH-57: "in the legend too"; FEA-CH-33), and keeps STY-40's plain swatch
//! where colour does. The runtime's buttons draw the same swatches (verify-report-runtime). On
//! paper the legend prints as plain text beside the vector chart (FEA-CH-67).

use regex::Regex;
use richview_core::document::{Output, RenderOptions, ReportFolder, render};

const SOURCE: &str = "---\ntitle: Legend\n---\n\n```chart\ntype: bar\ntitle: Attention by kind\nencoding: {x: pair, y: share, color: kind}\ndata:\n  inline:\n  - {pair: Google, kind: Products, share: 8}\n  - {pair: Google, kind: Companies, share: 23}\n  - {pair: Google, kind: Chief executives, share: 31}\n  - {pair: Wikipedia, kind: Products, share: 6}\n  - {pair: Wikipedia, kind: Companies, share: 55}\n  - {pair: Wikipedia, kind: Chief executives, share: 45}\n```\n";

fn html(style: &str, output: Output) -> String {
    render(
        SOURCE,
        &RenderOptions {
            styles: vec![style.to_owned()],
            output,
            report_folder: Some(ReportFolder {
                root: "legend-cues-fixture".into(),
                source: "report.rmd".into(),
                files: std::iter::once("report.rmd".into()).collect(),
            }),
            ..Default::default()
        },
    )
    .unwrap_or_else(|e| panic!("{style}: {e:?}"))
    .html
}

/// The swatches of the report's first static legend, each as its inner HTML.
fn swatches(style: &str) -> Vec<String> {
    let html = html(style, Output::Report);
    let legend = Regex::new(r#"(?s)<div class="rv-chart__legend[^"]*" role="group" aria-label="Series" data-rv-static>(.*?)</div>"#)
        .unwrap();
    let entries = legend.captures(&html).expect("a static legend")[1].to_owned();
    Regex::new(r#"(?s)<span class="rv-chart__swatch[^"]*"[^>]*>(.*?)</span><span class="rv-chart__legend-name">"#)
        .unwrap()
        .captures_iter(&entries)
        .map(|c| c[1].to_owned())
        .collect()
}

#[test]
fn hand_drawn_legend_swatches_show_each_series_hatch() {
    let all = swatches("hand-drawn");
    assert_eq!(all.len(), 3, "{all:?}");
    for svg in &all {
        assert!(
            svg.starts_with("<svg class=\"rv-chart__decal\""),
            "a plain swatch: {svg:?}"
        );
        assert!(svg.contains("<path d=\"M"), "no hatch: {svg}");
    }
    assert!(
        all[0] != all[1] && all[1] != all[2] && all[0] != all[2],
        "two series share one swatch"
    );
}

#[test]
fn colour_styles_keep_plain_swatches() {
    for style in ["sleek", "air", "scientific", "brutalist"] {
        let all = swatches(style);
        assert_eq!(all.len(), 3, "{style}: {all:?}");
        assert!(all.iter().all(String::is_empty), "{style}: {all:?}");
    }
}

#[test]
fn a_printed_chart_names_its_series_in_a_legend_in_every_style() {
    for style in richview_core::builtin_style_names() {
        let page = html(style, Output::Pdf);
        let figure = page
            .split("<figure class=\"rv-chart rv-chart--print\"")
            .nth(1)
            .and_then(|f| f.split("</figure>").next())
            .unwrap_or_else(|| panic!("{style}: a printed figure"));
        for name in ["Products", "Companies", "Chief executives"] {
            assert!(
                figure.contains(&format!(
                    "<span class=\"rv-chart__legend-name\">{name}</span>"
                )),
                "{style}: the printed chart names {name}"
            );
        }
        assert_eq!(
            figure.contains("<svg class=\"rv-chart__decal\""),
            *style == "hand-drawn",
            "{style}: the printed swatches carry the cue where decals are on"
        );
    }
}
