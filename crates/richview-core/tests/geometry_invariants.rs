//! Geometry dimensions stay valid before either renderer sees them (FEA-BR-19).

use richview_chart::block::{ChartBlock, ChartType};
use richview_chart::geometry::{Cmd, Geometry, Item, Rect};
use richview_chart::layout::{ChartTheme, Options, layout};
use richview_chart::payload::Payload;
use richview_core::document::{RenderOptions, ReportFolder, render};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn fixtures() -> Vec<Value> {
    let root = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../richview-chart/tests/fixtures/types"
    );
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    let mut fixtures: Vec<Value> = paths
        .iter()
        .map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap())
        .collect();
    // The original chart families live in the every-block golden report; later families
    // have individual JSON fixtures. Keep both real input collections in this matrix.
    let root = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/every-block"
    ));
    let doc =
        richview_core::document::parse(&std::fs::read_to_string(root.join("report.rmd")).unwrap())
            .unwrap();
    for block in doc.blocks.iter().filter(|b| b.tag == "chart") {
        if fixtures
            .iter()
            .any(|f| f["spec"]["type"] == block.value["type"])
        {
            continue;
        }
        let rows = if let Some(rows) = block
            .value
            .pointer("/data/inline")
            .and_then(Value::as_array)
        {
            rows.clone()
        } else {
            let path = block
                .value
                .pointer("/data/csv")
                .and_then(Value::as_str)
                .expect("golden CSV");
            let mut csv = csv::Reader::from_path(root.join(path)).unwrap();
            let names = csv.headers().unwrap().clone();
            csv.records()
                .map(|r| {
                    Value::Object(
                        names
                            .iter()
                            .zip(r.unwrap().iter())
                            .map(|(name, value)| (name.to_owned(), Value::String(value.to_owned())))
                            .collect(),
                    )
                })
                .collect()
        };
        fixtures.push(json!({"spec":block.value,"rows":rows}));
    }
    fixtures
}

fn theme(style: &str) -> ChartTheme {
    let report = render(
        "```chart\n{\"type\":\"bar\",\"title\":\"T\",\"encoding\":{\"x\":\"x\",\"y\":\"y\"},\"data\":{\"inline\":[{\"x\":\"A\",\"y\":1}]}}\n```",
        &RenderOptions {
            styles: vec![style.into()],
            report_folder: Some(ReportFolder {
                root: "geometry-invariants".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        },
    ).unwrap();
    let payload: Payload = serde_json::from_value(report.charts[0].clone()).unwrap();
    payload.theme
}

fn dimension(value: f64, at: &str) {
    assert!(
        value.is_finite() && value >= 0.0,
        "{at}: invalid dimension {value}"
    );
}

fn point(point: [f64; 2], at: &str) {
    assert!(
        point.into_iter().all(f64::is_finite),
        "{at}: invalid position {point:?}"
    );
}

fn rect(rect: Rect, at: &str) {
    point([rect.x, rect.y], at);
    dimension(rect.w, at);
    dimension(rect.h, at);
}

fn valid(g: &Geometry, at: &str) {
    dimension(g.width, at);
    dimension(g.height, at);
    for panel in &g.panels {
        rect(panel.frame, at);
        rect(panel.plot, at);
        for o in &panel.observations {
            point(o.at, at);
            assert!(o.x.is_finite(), "{at}: invalid observation x");
            dimension(o.radius, at);
            if let Some(area) = o.area {
                rect(area, at);
            }
            if let Some(wedge) = o.wedge {
                point(wedge.center, at);
                dimension(wedge.radius, at);
                dimension(wedge.inner, at);
                assert!(
                    wedge.start.is_finite() && wedge.end.is_finite(),
                    "{at}: invalid angles"
                );
            }
            for dot in &o.dots {
                point(dot.at, at);
            }
        }
    }
    for item in g.items() {
        let at = format!("{at}: {item:?}");
        if let Some(bounds) = item.bounds() {
            rect(bounds, &at);
        }
        match item {
            Item::Rect {
                rect: area,
                radius,
                stroke,
                ..
            } => {
                rect(*area, &at);
                for r in radius {
                    dimension(*r, &at);
                }
                if let Some(stroke) = stroke {
                    dimension(stroke.width, &at);
                }
            }
            Item::Point {
                center,
                radius,
                stroke,
                ..
            } => {
                point(*center, &at);
                dimension(*radius, &at);
                if let Some(stroke) = stroke {
                    dimension(stroke.width, &at);
                }
            }
            Item::Line { path, .. } | Item::Area { path, .. } => {
                if let Item::Line { stroke, .. } = item {
                    dimension(stroke.width, &at);
                }
                for cmd in &path.cmds {
                    match cmd {
                        Cmd::M { p } | Cmd::L { p } => point(*p, &at),
                        Cmd::C { c1, c2, p } => {
                            point(*c1, &at);
                            point(*c2, &at);
                            point(*p, &at);
                        }
                        Cmd::Z => {}
                    }
                }
            }
            Item::Text(text) => {
                point([text.x, text.y], &at);
                dimension(text.size, &at);
                rect(text.bounds, &at);
            }
        }
    }
    for check in &g.checks {
        if let Some(region) = check.region {
            rect(region, at);
        }
    }
    for dropped in &g.dropped {
        if let Some(region) = dropped.would_be_at {
            rect(region, at);
        }
    }
}

#[test]
fn every_chart_has_finite_nonnegative_geometry_in_real_styles_from_zero_width_up() {
    let fixtures = fixtures();
    let types: BTreeSet<_> = ChartType::all().map(ChartType::name).collect();
    let covered: BTreeSet<_> = fixtures
        .iter()
        .map(|f| f["spec"]["type"].as_str().unwrap())
        .collect();
    assert_eq!(covered, types, "every engine type needs a golden input");
    for style in ["air", "brutalist", "hand-drawn", "scientific", "sleek"] {
        let theme = theme(style);
        for fixture in &fixtures {
            let kind = fixture["spec"]["type"].as_str().unwrap();
            for compact in [false, true] {
                let mut spec = fixture["spec"].clone();
                if compact {
                    spec["style"]["ratio"] = json!(6);
                }
                let block = ChartBlock::from_value(&spec).unwrap();
                for width in [0.0, 1.0, 120.0, 240.0, 288.0, 320.0, 390.0, 1440.0] {
                    let options = Options {
                        width,
                        ..Default::default()
                    };
                    let rows = fixture["rows"].as_array().unwrap();
                    let at = format!("{kind}, {style}, width {width}, compact {compact}");
                    valid(&layout(&block, rows, &theme, &options), &at);
                }
            }
        }
    }
}
