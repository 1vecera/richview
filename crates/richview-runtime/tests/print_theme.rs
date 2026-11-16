//! Paper: a chart in a dark style prints in its light counterpart's theme, as the page around it
//! does, and returns to the screen's theme after printing (FEA-CH-67, STY-21). The charts are built
//! by core in Sleek, the default style, whose paper is Air.

use richview_chart::geometry::Geometry;
use richview_core::document::{RenderOptions, ReportFolder, render};
use richview_runtime::payload::Chart;
use serde_json::{Value, json};

fn chart_from(source: &str) -> Chart {
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "paper".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    let html = render(source, &options).expect("renders").html;
    let at = html.find("data-rv-payload>").expect("payload") + "data-rv-payload>".len();
    let json = &html[at..at + html[at..].find("</script>").unwrap()];
    Chart::parse(json).expect("payload parses")
}

fn chart_of(block: Value) -> Chart {
    chart_from(&format!("```chart\n{block}\n```\n"))
}

fn model(g: &Geometry) -> Value {
    serde_json::to_value(g).expect("the geometry serialises")
}

/// The same chart, made in the paper's theme from the start.
fn made_for_paper(chart: &Chart) -> Chart {
    let mut payload = chart.payload.clone();
    payload.theme = payload
        .print
        .take()
        .expect("a dark style carries its paper theme");
    Chart::from_payload(payload).expect("the paper payload parses")
}

#[test]
fn a_dark_styles_chart_prints_as_one_made_in_its_paper_theme_and_returns_after() {
    let rows = json!([
        {"region": "North", "q": "a", "v": 3}, {"region": "North", "q": "b", "v": 7},
        {"region": "South", "q": "a", "v": 40}, {"region": "South", "q": "b", "v": 25}
    ]);
    let mut chart = chart_of(json!({
        "type": "bar", "title": "T", "data": {"inline": rows},
        "encoding": {"x": "q", "y": "v", "color": "region"},
        "controls": [{"kind": "filter", "id": "r", "field": "region", "options": ["North", "South"]}]
    }));
    let print = chart.payload.print.clone().expect("Sleek prints as Air");
    assert_ne!(print, chart.payload.theme, "Sleek's screen theme is dark");
    // The reader filtered the chart before printing: the paper shows that state.
    chart.set_state(vec![1]);
    let screen = chart.geometry(640.0, &[], None);
    let mut paper = made_for_paper(&chart);
    paper.set_state(vec![1]);
    let expected = paper.geometry(640.0, &[], None);
    assert_ne!(expected.background, screen.background);

    assert!(chart.set_paper(true));
    assert!(
        !chart.set_paper(true),
        "print media after beforeprint changes nothing"
    );
    assert_eq!(model(&chart.geometry(640.0, &[], None)), model(&expected));
    assert_eq!(chart.payload.theme, print);

    assert!(chart.set_paper(false));
    assert!(
        !chart.set_paper(false),
        "afterprint after the media change changes nothing"
    );
    assert_eq!(model(&chart.geometry(640.0, &[], None)), model(&screen));
}

#[test]
fn a_light_styles_chart_prints_in_its_own_theme() {
    let block = json!({"type": "bar", "title": "T", "data": {"inline": [{"q": "a", "v": 3}]}, "encoding": {"x": "q", "y": "v"}});
    let mut chart = chart_from(&format!(
        "---\ntitle: T\nstyles: [air]\n---\n\n```chart\n{block}\n```\n"
    ));
    assert!(chart.payload.print.is_none(), "Air is its own paper");
    let screen = model(&chart.geometry(640.0, &[], None));
    assert!(!chart.set_paper(true));
    assert_eq!(model(&chart.geometry(640.0, &[], None)), screen);
    assert!(!chart.set_paper(false));
}
