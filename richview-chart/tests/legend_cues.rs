//! A legend repeats the cue that tells its series apart where decals are on (FEA-CH-57: "in the
//! legend too"; FEA-CH-33: "Swatches show the series' hatch where decals are on"). Hand-drawn
//! draws in greys, so its key is readable only if each swatch shows the series' hatch, dash and
//! marker as the plot draws them.

use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Geometry, Item, Role, Swatch};
use richview_chart::layout::{ChartTheme, Options, layout};
use richview_chart::vector::legend_swatch;
use serde_json::{Value, json};

fn draw(spec: Value, rows: &[Value], decals: bool, hidden: &[&str]) -> Geometry {
    let theme = ChartTheme {
        decals,
        ..ChartTheme::default()
    };
    let block = ChartBlock::from_value(&spec).expect("block");
    layout(
        &block,
        rows,
        &theme,
        &Options {
            width: 640.0,
            hidden: hidden.iter().map(|h| (*h).to_owned()).collect(),
            ..Options::default()
        },
    )
}

fn four_lines() -> Vec<Value> {
    let mut rows = Vec::new();
    for (k, name) in ["HN", "GitHub", "npm", "Google"].iter().enumerate() {
        for m in 1..=8 {
            rows.push(
                json!({"month": format!("2026-0{m}"), "signal": name, "share": 20 + 10 * k + (m * 7 + k * 3) % 9}),
            );
        }
    }
    rows
}

fn line_spec() -> Value {
    json!({"type": "line", "title": "T", "encoding": {"x": "month", "y": "share", "color": "signal"}})
}

fn swatches(g: &Geometry, swatch: Swatch) -> Vec<String> {
    (0..g.series.len())
        .map(|i| legend_swatch(g, i, swatch).unwrap_or_else(|| panic!("series {i} has no cue swatch")))
        .collect()
}

fn distinct(all: &[String]) {
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b, "two series share one swatch");
        }
    }
}

#[test]
fn each_line_swatch_shows_its_dash_and_vertex_marker() {
    let g = draw(line_spec(), &four_lines(), true, &[]);
    assert_eq!(g.series.len(), 4);
    let all = swatches(&g, Swatch::Line);
    distinct(&all);
    for (s, svg) in g.series.iter().zip(&all) {
        let dash: Vec<String> = s.dash.iter().map(|v| v.to_string()).collect();
        if dash.is_empty() {
            assert!(
                !svg.contains("stroke-dasharray"),
                "a solid series draws a solid swatch: {svg}"
            );
        } else {
            assert!(
                svg.contains(&format!("stroke-dasharray=\"{}\"", dash.join(" "))),
                "{}: the swatch carries the dash {dash:?}: {svg}",
                s.name
            );
        }
        // The plot draws a hollow vertex marker on each line; the swatch draws one too, in the
        // plot's fill (the chart surface) and the series colour.
        let marker = g
            .items()
            .find_map(|i| match i {
                Item::Point {
                    series,
                    stroke: Some(_),
                    fill: Some(fill),
                    ..
                } if *series == g.series.iter().position(|x| x.key == s.key) => Some(fill.color.hex()),
                _ => None,
            })
            .expect("the plot draws markers on decal lines");
        assert!(
            svg.contains(&format!("fill=\"{marker}\"")),
            "{}: hollow marker: {svg}",
            s.name
        );
        assert!(
            svg.contains(&format!("stroke=\"{}\"", s.color.hex())),
            "{}: series colour: {svg}",
            s.name
        );
    }
}

#[test]
fn each_bar_swatch_shows_its_hatch_as_the_bars_do() {
    let rows: Vec<Value> = ["Google", "Wikipedia"]
        .iter()
        .flat_map(|x| {
            ["Products", "Companies", "Chief executives"]
                .iter()
                .enumerate()
                .map(move |(k, c)| json!({"pair": x, "kind": c, "share": 10 + 15 * k}))
        })
        .collect();
    let g = draw(
        json!({"type": "bar", "title": "T", "encoding": {"x": "pair", "y": "share", "color": "kind"}}),
        &rows,
        true,
        &[],
    );
    let all = swatches(&g, Swatch::Square);
    distinct(&all);
    for (i, svg) in all.iter().enumerate() {
        let (color, ink) = g
            .items()
            .find_map(|item| match item {
                Item::Rect {
                    fill,
                    series,
                    role: Role::Mark,
                    ..
                } if *series == Some(i) && fill.hatch.is_some() => {
                    Some((fill.color.hex(), fill.hatch_ink.expect("ink").hex()))
                }
                _ => None,
            })
            .expect("decal bars are hatched");
        assert!(
            svg.contains(&format!("<rect width=\"12\" height=\"12\" fill=\"{color}\"")),
            "{svg}"
        );
        assert!(
            svg.contains(&format!("stroke=\"{ink}\"")),
            "the hatch in the bars' ink: {svg}"
        );
    }
}

#[test]
fn an_area_swatch_draws_its_hatch_at_the_plots_opacity_without_a_group() {
    let g = draw(
        json!({"type": "area", "title": "T", "encoding": {"x": "month", "y": "share", "color": "signal"}}),
        &four_lines(),
        true,
        &[],
    );
    let all = swatches(&g, Swatch::Square);
    distinct(&all);
    for svg in &all {
        // A group's opacity prints as a transparency group, not as the vector paths the PDF
        // check reads (verify-chart-type-pdf); one stroke's opacity prints as a path.
        assert!(!svg.contains(" opacity="), "{svg}");
        assert!(
            svg.contains("stroke-width=\"1\" stroke-linecap=\"butt\"") && svg.contains("stroke-opacity=\"0.6\""),
            "the hatch at 0.6, as the plot draws an area's: {svg}"
        );
    }
}

#[test]
fn each_point_swatch_shows_its_marker_shape() {
    let rows: Vec<Value> = (0..30)
        .map(|k| {
            let g = ["A", "B", "C"][k % 3];
            json!({"x": k, "y": (k * 7) % 11, "g": g})
        })
        .collect();
    let g = draw(
        json!({"type": "scatter", "title": "T", "encoding": {"x": "x", "y": "y", "color": "g"}}),
        &rows,
        true,
        &[],
    );
    let all = swatches(&g, Swatch::Dot);
    distinct(&all);
}

#[test]
fn a_hidden_series_keeps_the_swatch_it_showed() {
    let shown = draw(line_spec(), &four_lines(), true, &[]);
    let hidden = draw(line_spec(), &four_lines(), true, &["npm"]);
    let npm = hidden.series.iter().position(|s| s.key == "npm").expect("npm");
    assert!(hidden.series[npm].hidden);
    assert!(
        !hidden
            .items()
            .any(|i| matches!(i, Item::Line { series, .. } if *series == Some(npm))),
        "a hidden series draws no marks"
    );
    assert_eq!(
        legend_swatch(&hidden, npm, Swatch::Line),
        legend_swatch(&shown, npm, Swatch::Line),
        "the hidden series' swatch is drawn from another series in its own colour and cue"
    );
}

#[test]
fn without_decals_the_legend_keeps_its_plain_swatches() {
    let g = draw(line_spec(), &four_lines(), false, &[]);
    for i in 0..g.series.len() {
        assert_eq!(legend_swatch(&g, i, Swatch::Line), None);
    }
}

#[test]
fn readout_series_rows_are_distinguished_from_statistic_rows() {
    let lines = draw(line_spec(), &four_lines(), false, &[]);
    let line_rows = &lines.panels[0].observations[0].readout.lines;
    assert!(line_rows.len() > 1 && line_rows.iter().all(|row| row.names_series));

    let combo = json!({"type":"combo","title":"T","encoding":{"x":"quarter","y":"revenue","line":"margin"}});
    let rows = [
        json!({"quarter":"Q1","revenue":100,"margin":10}),
        json!({"quarter":"Q2","revenue":150,"margin":20}),
    ];
    let combo = draw(combo, &rows, false, &[]);
    assert!(
        combo.panels[0].observations[0]
            .readout
            .lines
            .iter()
            .all(|row| row.names_series)
    );

    for fixture in [
        include_str!("fixtures/types/band.json"),
        include_str!("fixtures/types/dumbbell.json"),
        include_str!("fixtures/types/violin.json"),
    ] {
        let fixture: Value = serde_json::from_str(fixture).unwrap();
        let geometry = draw(fixture["spec"].clone(), fixture["rows"].as_array().unwrap(), false, &[]);
        let rows = &geometry.panels[0].observations[0].readout.lines;
        assert!(rows.iter().all(|row| !row.names_series), "{rows:?}");
    }

    let fixture: Value = serde_json::from_str(include_str!("fixtures/types/streamgraph.json")).unwrap();
    let graph = draw(fixture["spec"].clone(), fixture["rows"].as_array().unwrap(), false, &[]);
    let rows = &graph.panels[0].observations[0].readout.lines;
    assert!(
        rows.iter()
            .filter(|row| row.name != "Total")
            .all(|row| row.names_series)
    );
    assert!(
        rows.iter()
            .filter(|row| row.name == "Total")
            .all(|row| !row.names_series)
    );
}
