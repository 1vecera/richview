//! Flow charts written as cards and edges (FEA-CH-76): the core gives them their Table rows,
//! refuses the flow fields elsewhere, and the example draws clean in every built-in style. A
//! flow drawn from data rows, as the every-block report writes it, draws every step legibly.

use richview_chart::block::ChartBlock;
use richview_chart::geometry::Rect;
use richview_chart::layout::{Options, layout};
use richview_chart::payload::Payload;
use richview_core::document::{RenderOptions, ReportFolder, render};
use serde_json::Value;

const STYLES: [&str; 5] = ["air", "brutalist", "hand-drawn", "scientific", "sleek"];

fn example() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/flow/report.rmd"
    ))
    .unwrap()
}

/// The flow chart of the every-block report, a flow drawn from data rows, as a report of its own.
fn every_block_flow() -> String {
    let report = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/every-block/report.rmd"
    ))
    .unwrap();
    let open = "```chart\ntype: flow\n";
    let start = report.find(open).expect("every-block has a flow chart");
    let close = "\n```\n";
    let end = start + report[start..].find(close).expect("the block ends") + close.len();
    format!(
        "---\ntitle: A flow from data rows\ndescription: The every-block flow chart.\nversion: 0.0.1\n---\n\n## Flow\n\n{}",
        &report[start..end]
    )
}

fn options(style: &str) -> RenderOptions {
    RenderOptions {
        styles: vec![style.into()],
        report_folder: Some(ReportFolder {
            root: "flow-chart".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    }
}

fn codes(source: &str) -> Vec<(String, String)> {
    let doc = richview_core::document::parse(source).unwrap();
    doc.diagnostics
        .iter()
        .map(|d| (d.code.clone(), d.path.clone()))
        .collect()
}

#[test]
fn a_flow_written_as_cards_renders_with_its_table_rows() {
    let report = render(&example(), &options("scientific")).unwrap();
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.level == "error" || d.level == "warning")
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let payload: Payload = serde_json::from_value(report.charts[0].clone()).unwrap();
    assert_eq!(payload.block["type"], "flow");
    assert!(
        payload.block.get("nodes").is_some(),
        "the runtime lays the cards out again"
    );
    // One row per card, then one per edge.
    assert_eq!(payload.rows.len(), 13 + 15);
    assert!(payload.columns.iter().any(|c| c == "step"));
    assert!(payload.columns.iter().any(|c| c == "label"));
}

#[test]
fn the_flow_fields_belong_to_a_flow() {
    let bar = "```chart\ntype: bar\ntitle: T\nnodes: [{title: A}]\ndata: {inline: [{x: a, y: 1}]}\nencoding: {x: x, y: y}\n```\n";
    assert!(
        codes(bar)
            .iter()
            .any(|(c, p)| c == "block.schema" && p == "chart.nodes")
    );
    let both = "```chart\ntype: flow\ntitle: T\nnodes: [{title: A}]\ndata: {inline: [{name: A}]}\nencoding: {label: name}\n```\n";
    assert!(
        codes(both)
            .iter()
            .any(|(c, p)| c == "block.schema" && p == "chart.nodes")
    );
    let kind = "```chart\ntype: flow\ntitle: T\nnodes: [{title: A, kind: rhombus}]\n```\n";
    assert!(
        codes(kind).iter().any(|(_, p)| p == "chart.nodes[0].kind"),
        "{:?}",
        codes(kind)
    );
    let fine =
        "```chart\ntype: flow\ntitle: T\nnodes: [{title: A}, {title: B}]\nedges: [A -> B]\n```\n";
    assert!(codes(fine).is_empty(), "{:?}", codes(fine));
}

/// The example in every built-in style at a phone's and a report's width: nothing reported,
/// nothing dropped. `RICHVIEW_GALLERY_OUT=<dir>` writes each as SVG for review.
#[test]
fn the_example_draws_clean_in_every_style() {
    for style in STYLES {
        let report = render(&example(), &options(style)).unwrap();
        let payload: Payload = serde_json::from_value(report.charts[0].clone()).unwrap();
        let block = ChartBlock::from_value(&payload.block).unwrap();
        let rows: Vec<Value> = richview_chart::layout::flow_table_rows(&payload.block).unwrap();
        for width in [390.0, 720.0] {
            let g = layout(
                &block,
                &rows,
                &payload.theme,
                &Options {
                    width,
                    ..Options::default()
                },
            );
            assert!(g.failure.is_none(), "{style} {width}");
            assert!(g.checks.is_empty(), "{style} {width}: {:?}", g.checks);
            assert!(g.dropped.is_empty(), "{style} {width}: {:?}", g.dropped);
            if let Ok(dir) = std::env::var("RICHVIEW_GALLERY_OUT") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    format!("{dir}/flow-{style}-{width}.svg"),
                    richview_chart::vector::svg(&g, &payload.theme),
                )
                .unwrap();
            }
        }
    }
}

/// A flow drawn from data rows, as the every-block report writes it, in every built-in style at
/// a phone's, a report's and a desktop's width: every step is a card a reader can read. It once
/// drew as an empty strip with no check and no failure, at every width.
#[test]
fn a_flow_from_data_rows_draws_every_step_in_every_style() {
    for style in STYLES {
        let report = render(&every_block_flow(), &options(style)).unwrap();
        let payload: Payload = serde_json::from_value(report.charts[0].clone()).unwrap();
        assert_eq!(payload.block["type"], "flow");
        assert!(payload.block.get("nodes").is_none(), "drawn from its rows");
        let block = ChartBlock::from_value(&payload.block).unwrap();
        let rows = payload.objects();
        for width in [390.0, 632.0, 1280.0] {
            let g = layout(
                &block,
                &rows,
                &payload.theme,
                &Options {
                    width,
                    ..Options::default()
                },
            );
            let at = format!("{style} at {width} px");
            assert!(g.failure.is_none(), "{at}: {:?}", g.failure);
            assert!(g.checks.is_empty(), "{at}: {:?}", g.checks);
            let cards: Vec<(String, Rect)> = g
                .panels
                .iter()
                .flat_map(|p| &p.observations)
                .filter(|o| !o.readout.heading.contains(" → "))
                .filter_map(|o| o.area.map(|a| (o.readout.heading.clone(), a)))
                .collect();
            assert_eq!(cards.len(), 7, "{at}: every step is drawn");
            for (name, c) in &cards {
                assert!(
                    c.w >= 40.0 && c.h >= 14.0,
                    "{at}: '{name}' is drawn {:.1} × {:.1} px",
                    c.w,
                    c.h
                );
            }
            assert!(
                g.height >= 100.0,
                "{at}: the figure is {:.0} px tall",
                g.height
            );
            if let Ok(dir) = std::env::var("RICHVIEW_GALLERY_OUT") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    format!("{dir}/rows-{style}-{width}.svg"),
                    richview_chart::vector::svg(&g, &payload.theme),
                )
                .unwrap();
            }
        }
    }
}
