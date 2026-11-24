//! A slide's stage sets a chart's lettering as a share of its width (FEA-RP-65). The chart is laid
//! out again at that size, as one made at it from the start, on the screen and on paper, and takes
//! its style's own size back when it leaves the stage.

use richview_chart::geometry::Geometry;
use richview_core::document::{RenderOptions, ReportFolder, render};
use richview_runtime::payload::Chart;
use serde_json::{Value, json};

fn chart() -> Chart {
    let rows = json!([
        {"region": "North", "q": "a", "v": 3}, {"region": "North", "q": "b", "v": 7},
        {"region": "South", "q": "a", "v": 40}, {"region": "South", "q": "b", "v": 25}
    ]);
    let block = json!({
        "type": "bar", "title": "T", "data": {"inline": rows},
        "encoding": {"x": "q", "y": "v", "color": "region"},
        "controls": [{"kind": "filter", "id": "r", "field": "region", "options": ["North", "South"]}]
    });
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "stage".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    let html = render(&format!("```chart\n{block}\n```\n"), &options)
        .expect("renders")
        .html;
    let at = html.find("data-rv-payload>").expect("payload") + "data-rv-payload>".len();
    let json = &html[at..at + html[at..].find("</script>").unwrap()];
    Chart::parse(json).expect("payload parses")
}

fn model(g: &Geometry) -> Value {
    serde_json::to_value(g).expect("the geometry serialises")
}

/// The same chart with `size` as its lettering from the start, in the reader's state.
fn made_at(chart: &Chart, size: f64) -> Chart {
    let mut payload = chart.payload.clone();
    payload.theme.size = size;
    if let Some(print) = &mut payload.print {
        print.size = size;
    }
    let mut made = Chart::from_payload(payload).expect("the payload parses");
    made.set_state(chart.state.clone());
    made
}

#[test]
fn a_stage_sets_the_lettering_and_the_style_takes_it_back() {
    let mut chart = chart();
    // The reader filtered the chart before it met the stage: the state stays.
    chart.set_state(vec![1]);
    let own = chart.geometry(640.0, &[], None);
    let own_paper = chart.payload.print.as_ref().map(|print| print.size);
    let staged = made_at(&chart, 21.0).geometry(640.0, &[], None);
    assert_ne!(
        model(&staged),
        model(&own),
        "larger lettering is another layout"
    );

    assert!(chart.set_text_size(Some(21.0)));
    assert!(
        !chart.set_text_size(Some(21.0)),
        "the same size changes nothing"
    );
    assert_eq!(model(&chart.geometry(640.0, &[], None)), model(&staged));

    assert!(chart.set_text_size(None));
    assert!(!chart.set_text_size(None));
    assert_eq!(model(&chart.geometry(640.0, &[], None)), model(&own));
    assert_eq!(
        chart.payload.print.as_ref().map(|print| print.size),
        own_paper
    );
}

#[test]
fn paper_keeps_the_stages_lettering_in_both_themes() {
    let mut chart = chart();
    let own = chart.geometry(640.0, &[], None);
    assert!(chart.set_text_size(Some(18.0)));
    let screen = chart.geometry(640.0, &[], None);
    // Sleek prints as Air: the paper theme takes the stage's size too.
    assert!(chart.set_paper(true));
    assert_eq!(chart.payload.theme.size, 18.0);
    // The window was resized while the print dialog stood open.
    assert!(chart.set_text_size(Some(24.0)));
    assert_eq!(chart.payload.theme.size, 24.0);
    assert!(chart.set_paper(false));
    assert_eq!(chart.payload.theme.size, 24.0);
    assert!(chart.set_text_size(Some(18.0)));
    assert_eq!(model(&chart.geometry(640.0, &[], None)), model(&screen));
    assert!(chart.set_text_size(None));
    assert_eq!(model(&chart.geometry(640.0, &[], None)), model(&own));
}
