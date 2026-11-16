//! Core's row/grid JSON contract as the runtime reads it. The checked-in values are calculated
//! independently: x has native unit steps; y spans 87 hundredths with 47 steps per hundredth.
//! The derived fixture fixes a normalized grid's wire shape and its selected-state replay;
//! the individual layout codecs retain their own mathematical contracts.

use richview_core::document::{RenderOptions, ReportFolder, render};
use richview_runtime::payload::Chart;
use serde_json::{Value, json};

const ROWS_V1: &str = include_str!("fixtures/payload-rows-v1.json");

fn native_payload() -> String {
    let source = r#"```chart
{
  "type": "scatter", "title": "Measured values", "digits": 3,
  "encoding": {"x": "x", "y": "y"},
  "data": {"inline": [
    {"x": 100, "y": 0.1234},
    {"x": 101, "y": null},
    {"x": 102, "y": 0.9876}
  ]}
}
```
"#;
    payload_from_source(source)
}

fn payload_from_source(source: &str) -> String {
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "payload-contract".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    let html = render(source, &options).expect("report renders").html;
    let start = html.find("data-rv-payload>").expect("embedded payload") + "data-rv-payload>".len();
    html[start..].split_once("</script>").unwrap().0.to_owned()
}

#[test]
fn selected_state_derived_grids_keep_their_wire_shape_and_drive_runtime_geometry() {
    // Both cohorts have exact quarter/half shares, in different orders. Recomputing from the
    // three-digit shown rows changes those shares; the exact-state record must cross the wire.
    let block = json!({
        "type": "pie", "title": "Ratios by cohort", "digits": 3,
        "encoding": {"x": "name", "y": "v"},
        "data": {"inline": [
            {"cohort": "North", "name": "A", "v": 1.2344},
            {"cohort": "North", "name": "B", "v": 2.4688},
            {"cohort": "North", "name": "C", "v": 1.2344},
            {"cohort": "South", "name": "A", "v": 1.2344},
            {"cohort": "South", "name": "B", "v": 1.2344},
            {"cohort": "South", "name": "C", "v": 2.4688}
        ]},
        "controls": [{"kind": "filter", "id": "cohort", "field": "cohort", "options": ["North", "South"]}]
    });
    let expected = json!([
        {"state": [0], "derived": [{"panel": 0, "key": "pie.angles", "value": {
            "grid": {"min": 0.0, "max": 1.0, "steps": 4095},
            "edges": [[0, 1024], [1024, 3071], [3071, 4095]]
        }}]},
        {"state": [1], "derived": [{"panel": 0, "key": "pie.angles", "value": {
            "grid": {"min": 0.0, "max": 1.0, "steps": 4095},
            "edges": [[0, 1024], [1024, 2048], [2048, 4095]]
        }}]}
    ]);
    let wire = payload_from_source(&format!("```chart\n{block}\n```\n"));
    let encoded: Value = serde_json::from_str(&wire).unwrap();
    let states = expected.as_array().unwrap();
    let mut chart = Chart::parse(&wire).unwrap();
    for wanted in states.iter().chain(states.iter().take(1)) {
        let state = &wanted["state"];
        let container = encoded["derived"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| &d["state"] == state)
            .expect("one exact-state container crosses the wire");
        let records: Vec<_> = container["derived"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["panel"] == 0 && d["key"] == "pie.angles")
            .collect();
        assert_eq!(
            json!({"state": container["state"], "derived": records}),
            *wanted
        );
        chart.set_state(serde_json::from_value(state.clone()).unwrap());
        for width in [640.0, 390.0] {
            let geometry = chart.geometry(width, &[], None);
            assert!(
                geometry.failure.is_none(),
                "{state}@{width}: {:?}",
                geometry.failure
            );
            let observations: Vec<_> = geometry
                .panels
                .iter()
                .flat_map(|p| &p.observations)
                .collect();
            assert_eq!(observations.len(), 3);
            for observation in observations {
                let edges = &wanted["derived"][0]["value"]["edges"][observation.series.unwrap()];
                let angle =
                    |at| edges[at].as_u64().unwrap() as f64 / 4095.0 * std::f64::consts::TAU;
                let wedge = observation.wedge.unwrap();
                assert_eq!(
                    wedge.start,
                    angle(0),
                    "{state}@{width}: start comes from the selected record"
                );
                assert_eq!(
                    wedge.end,
                    angle(1),
                    "{state}@{width}: end comes from the selected record"
                );
            }
        }
    }
}

#[test]
fn native_encoding_keeps_the_row_grid_wire_contract() {
    let actual: Value = serde_json::from_str(&native_payload()).unwrap();
    let expected: Value = serde_json::from_str(ROWS_V1).unwrap();
    for (key, value) in expected.as_object().unwrap() {
        assert_eq!(&actual[key], value, "payload field {key}");
    }
}

#[test]
fn runtime_decodes_grid_positions_and_reads_shown_values_without_filling_missing_cells() {
    // The fixed wire fixture also exercises runtime defaults without producer-only themes or
    // readout caches. The full native payload must produce the same reader-visible values.
    for (name, wire) in [
        ("v1 fixture", ROWS_V1.to_owned()),
        ("native", native_payload()),
    ] {
        let chart = Chart::parse(&wire).expect("runtime reads payload");
        assert_eq!(
            chart.payload.shown_rows(),
            vec![
                vec![json!(100), json!(0.123)],
                vec![json!(101), Value::Null],
                vec![json!(102), json!(0.988)]
            ],
            "{name}: table/CSV values"
        );
        assert_eq!(chart.objects.len(), 3, "{name}: missing row keeps its slot");
        assert_eq!(chart.objects[1], json!({"x": 101.0, "y": null}));
        for (row, x, y) in [
            (0, 100.0, 0.12340425531914893),
            (2, 102.0, 0.9876595744680852),
        ] {
            assert_eq!(
                chart.objects[row]["x"],
                json!(x),
                "{name}: native x position"
            );
            assert!(
                (chart.objects[row]["y"].as_f64().unwrap() - y).abs() < 1e-14,
                "{name}: y position comes from the grid, not shown values"
            );
        }
        for width in [640.0, 390.0] {
            let geometry = chart.geometry(width, &[], None);
            assert!(
                geometry.failure.is_none(),
                "{name}@{width}: {:?}",
                geometry.failure
            );
            let values: Vec<_> = geometry
                .panels
                .iter()
                .flat_map(|p| &p.observations)
                .map(|o| {
                    let read = |field| {
                        o.readout
                            .lines
                            .iter()
                            .find(|line| line.name == field)
                            .unwrap()
                            .value
                            .as_str()
                    };
                    (read("X"), read("Y"))
                })
                .collect();
            assert_eq!(
                values,
                [("100", "0.123"), ("102", "0.988")],
                "{name}@{width}: two observed marks, no missing-value mark or grid readout"
            );
        }
    }
}

#[test]
fn series_name_flag_crosses_the_payload_and_defaults_for_older_reports() {
    let wire = payload_from_source(
        r#"```chart
{"type":"line","title":"T","encoding":{"x":"x","y":"v","color":"group"},"data":{"inline":[{"x":1,"v":12,"group":"North"},{"x":1,"v":14,"group":"South"},{"x":2,"v":13,"group":"North"},{"x":2,"v":15,"group":"South"}]}}
```
"#,
    );
    let value: Value = serde_json::from_str(&wire).unwrap();
    let text = value["text"].as_array().expect("embedded readouts");
    let lines = text[0]["lines"].as_array().expect("series rows");
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/payload-series-rows-v1.json")).unwrap();
    assert_eq!(lines, expected.as_array().unwrap());
    let chart = Chart::parse(&wire).unwrap();
    let geometry = chart.geometry(640.0, &[], None);
    let lines = &geometry.panels[0].observations[0].readout.lines;
    assert_eq!(lines.iter().filter(|line| line.names_series).count(), 2);
    let old: richview_chart::payload::TextLine =
        serde_json::from_value(json!({"name":"Value","value":"12","derived":false,"series":0}))
            .unwrap();
    assert!(!old.names_series);
}
