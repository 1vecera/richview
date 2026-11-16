//! Flow charts written as cards and edges (FEA-CH-76): the core gives them their Table rows,
//! refuses the flow fields elsewhere, and the example and a denser release flow (three
//! decisions in a row, a group, a loop back) draw clean in every built-in style. A flow
//! drawn from data rows, as the every-block report writes it, draws every step legibly.

use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Cmd, Geometry, Item, Rect, Role};
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

fn release_gates() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../richview-chart/tests/fixtures/flow/release-gates.rmd"
    ))
    .unwrap()
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

/// Places where a solid line steps sideways by 2 px or less between two straight runs.
fn jogs(g: &Geometry) -> Vec<f64> {
    let mut out = Vec::new();
    for item in g.items() {
        let Item::Line {
            path,
            stroke,
            role: Role::Mark,
            obs: Some(_),
            ..
        } = item
        else {
            continue;
        };
        if !stroke.dash.is_empty() {
            continue;
        }
        let pts: Vec<[f64; 2]> = path
            .cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::M { p } | Cmd::L { p } | Cmd::C { p, .. } => Some(*p),
                Cmd::Z => None,
            })
            .collect();
        for axis in 0..2 {
            // Runs along the other axis, and how far apart consecutive ones stand.
            let at: Vec<f64> = pts
                .windows(2)
                .filter(|w| {
                    (w[0][axis] - w[1][axis]).abs() < 0.01
                        && (w[0][1 - axis] - w[1][1 - axis]).abs() > 0.01
                })
                .map(|w| w[0][axis])
                .collect();
            out.extend(
                at.windows(2)
                    .map(|w| (w[0] - w[1]).abs())
                    .filter(|d| *d > 0.01 && *d <= 2.0),
            );
        }
    }
    out
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

/// The example and the release flow in every built-in style at a phone's and a report's width:
/// nothing reported, nothing dropped. `RICHVIEW_GALLERY_OUT=<dir>` writes each as SVG for review.
#[test]
fn the_examples_draw_clean_in_every_style() {
    for (name, source, style) in STYLES
        .iter()
        .flat_map(|s| [("flow", example(), *s), ("release", release_gates(), *s)])
    {
        let report = render(&source, &options(style)).unwrap();
        let payload: Payload = serde_json::from_value(report.charts[0].clone()).unwrap();
        let block = ChartBlock::from_value(&payload.block).unwrap();
        let rows: Vec<Value> = richview_chart::layout::flow_table_rows(&payload.block).unwrap();
        // The smallest and the largest text at each width.
        let mut sizes: Vec<(f64, f64)> = Vec::new();
        for width in [320.0, 390.0, 720.0] {
            let g = layout(
                &block,
                &rows,
                &payload.theme,
                &Options {
                    width,
                    ..Options::default()
                },
            );
            assert!(g.failure.is_none(), "{name} {style} {width}");
            assert!(
                g.checks.is_empty(),
                "{name} {style} {width}: {:?}",
                g.checks
            );
            assert!(
                g.dropped.is_empty(),
                "{name} {style} {width}: {:?}",
                g.dropped
            );
            assert!(
                jogs(&g).is_empty(),
                "{name} {style} {width}: lines jog {:?}",
                jogs(&g)
            );
            let text: Vec<f64> = g
                .panels
                .iter()
                .flat_map(|p| &p.items)
                .filter_map(|i| match i {
                    Item::Text(t) => Some(t.size),
                    _ => None,
                })
                .collect();
            sizes.push((
                text.iter().copied().fold(f64::INFINITY, f64::min),
                text.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            ));
            // A card's title takes three lines at most: titles are the largest text.
            let title = sizes.last().unwrap().1;
            for card in g
                .panels
                .iter()
                .flat_map(|p| &p.observations)
                .filter(|o| !o.readout.heading.contains(" → "))
            {
                let Some(area) = card.area else { continue };
                let lines = g
                    .panels
                    .iter()
                    .flat_map(|p| &p.items)
                    .filter(|i| matches!(i, Item::Text(t) if t.size >= title - 0.01 && t.bounds.inside(&area, 1.0)))
                    .count();
                assert!(
                    lines <= 3,
                    "{name} {style} {width}: '{}' sets its title in {lines} lines",
                    card.readout.heading
                );
            }
            if let Ok(dir) = std::env::var("RICHVIEW_GALLERY_OUT") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    format!("{dir}/{name}-{style}-{width}.svg"),
                    richview_chart::vector::svg(&g, &payload.theme),
                )
                .unwrap();
            }
        }
        // On a phone the type holds its floor: titles at most 2 px under the style's, and no
        // text more than 1 px under its smallest.
        let (phone, report) = (sizes[1], sizes[2]);
        assert!(
            phone.0 >= report.0 - 1.05 && phone.1 >= report.1 - 2.05,
            "{name} {style}: text of {:.2} to {:.2} px at 390 px, {:.2} to {:.2} px at 720 px",
            phone.0,
            phone.1,
            report.0,
            report.1
        );
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
        for width in [320.0, 390.0, 632.0, 1280.0] {
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
