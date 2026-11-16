//! What a reader sees in every state of a chart that core embedded: the runtime lays out each
//! filter option, playback frame, hidden series and zoom from the shown rows (TYP-36), so no
//! label, key or readout carries a number the grid made up. The charts are built by core, grid
//! and all, and walked the way the browser walks them.

use richview_chart::geometry::{Geometry, Item};
use richview_core::document::{RenderOptions, ReportFolder, render};
use richview_runtime::payload::Chart;
use serde_json::json;

fn chart_of(block: serde_json::Value) -> Chart {
    let source = format!("```chart\n{block}\n```\n");
    chart_from_source(&source)
}

fn chart_from_source(source: &str) -> Chart {
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "states".into(),
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

/// Every number-looking text the reader can read from `g`: readouts, and printed labels and keys.
fn texts(g: &Geometry) -> Vec<String> {
    let readouts = g
        .panels
        .iter()
        .flat_map(|p| &p.observations)
        .flat_map(|o| o.readout.lines.iter().map(|l| l.value.clone()));
    let printed = g.items().filter_map(|i| match i {
        Item::Text(t) if t.is_value_text() => Some(t.text.clone()),
        _ => None,
    });
    readouts.chain(printed).collect()
}

/// Author values 3, 7, 12, 40 and 250 sit off the 4,096-step grid; their grid neighbours (the
/// near misses below) must never print.
const NEAR_MISSES: [&str; 6] = ["2.95", "7.08", "7.01", "12.2", "39.9", "249"];

fn rows() -> serde_json::Value {
    json!([
        {"region": "North", "q": "a", "v": 3}, {"region": "North", "q": "b", "v": 7},
        {"region": "North", "q": "c", "v": 12}, {"region": "South", "q": "a", "v": 40},
        {"region": "South", "q": "b", "v": 250}, {"region": "South", "q": "c", "v": 7}
    ])
}

fn assert_clean(label: &str, g: &Geometry) {
    assert!(g.failure.is_none(), "{label}: {:?}", g.failure);
    for text in texts(g) {
        assert!(
            !NEAR_MISSES.contains(&text.as_str()),
            "{label}: {text:?} is a grid value"
        );
    }
}

#[test]
fn a_filtered_funnel_and_heatmap_read_author_values_in_every_state() {
    for (kind, encoding) in [
        ("funnel", json!({"x": "q", "y": "v"})),
        ("heatmap", json!({"x": "q", "y": "region", "color": "v"})),
        ("bar", json!({"x": "q", "y": "v", "color": "region"})),
    ] {
        let mut chart = chart_of(json!({
            "type": kind, "title": "T", "data": {"inline": rows()}, "encoding": encoding,
            "controls": [{"kind": "filter", "id": "r", "field": "region", "options": ["North", "South"]}]
        }));
        assert_clean(
            &format!("{kind} initial"),
            &chart.geometry(640.0, &[], None),
        );
        assert_clean(&format!("{kind} phone"), &chart.geometry(360.0, &[], None));
        chart.set_state(vec![1]);
        assert_clean(
            &format!("{kind} filtered"),
            &chart.geometry(640.0, &[], None),
        );
        assert_clean(
            &format!("{kind} hidden"),
            &chart.geometry(640.0, &["a".into()], None),
        );
    }
}

#[test]
fn bullet_ranges_keep_distinct_values_after_payload_replay() {
    let chart = chart_of(json!({
        "type": "bullet", "title": "Sales", "encoding": {"x": "region", "y": "sales", "target": "goal"},
        "style": {"ranges": [60, 80, 100]},
        "data": {"inline": [{"region": "East", "sales": 55, "goal": 70}]}
    }));
    for width in [360.0, 640.0] {
        let geometry = chart.geometry(width, &[], None);
        let lines = &geometry.panels[0].observations[0].readout.lines;
        let ranges: Vec<_> = lines.iter().filter(|line| line.name == "Ranges").collect();
        assert_eq!(ranges.len(), 1, "width {width}");
        assert_eq!(ranges[0].value, "60 · 80 · 100", "width {width}");
    }
}

#[test]
fn a_zoomed_line_and_a_playback_frame_read_author_values() {
    let rows: Vec<_> = (0..40)
        .map(|i| json!({"t": i, "year": if i < 20 { "2000" } else { "2001" }, "y": 3 + (i % 5) * 37}))
        .collect();
    let mut chart = chart_of(json!({
        "type": "line", "title": "T", "data": {"inline": rows}, "encoding": {"x": "t", "y": "y"},
        "controls": [{"kind": "playback", "id": "yr", "field": "year"}]
    }));
    assert_clean("line", &chart.geometry(640.0, &[], None));
    assert_clean("zoom", &chart.geometry(640.0, &[], Some([10.0, 25.0])));
    chart.set_state(vec![0]);
    assert_clean("frame", &chart.geometry(640.0, &[], None));
}

/// The drawn height of each observation's box beside the readout's count.
fn heights_and_counts(g: &Geometry) -> Vec<(f64, f64)> {
    g.panels
        .iter()
        .flat_map(|p| &p.observations)
        .filter_map(|o| {
            let count = o
                .readout
                .lines
                .iter()
                .find(|l| l.name == "Count")?
                .value
                .replace(',', "")
                .parse::<f64>()
                .ok()?;
            Some((o.area?.h, count))
        })
        .collect()
}

#[test]
fn a_filtered_histogram_draws_the_bars_its_readouts_name() {
    // 9.996 is 9.99 on the grid but "10" at three digits: a layout that derives bins twice
    // would put it in different bins for the bars and for the text.
    let rows = json!([
        {"region": "North", "v": 1}, {"region": "North", "v": 2}, {"region": "North", "v": 15}, {"region": "North", "v": 5000},
        {"region": "South", "v": 1}, {"region": "South", "v": 2}, {"region": "South", "v": 9.996},
        {"region": "South", "v": 11}, {"region": "South", "v": 12}, {"region": "South", "v": 25}
    ]);
    let mut chart = chart_of(json!({
        "type": "histogram", "title": "T", "data": {"inline": rows}, "encoding": {"x": "v"},
        "binning": {"edges": [0, 10, 20, 30]},
        "controls": [{"kind": "filter", "id": "r", "field": "region", "options": ["North", "South"]}]
    }));
    for state in [vec![0], vec![1]] {
        chart.set_state(state.clone());
        let g = chart.geometry(640.0, &[], None);
        let counts: Vec<_> = heights_and_counts(&g).iter().map(|p| p.1).collect();
        assert_eq!(
            counts,
            if state == [0] {
                vec![2.0, 1.0, 0.0]
            } else {
                vec![3.0, 2.0, 1.0]
            }
        );
        // An empty interval keeps a hover strip, so only intervals with a count are compared.
        let pairs: Vec<_> = heights_and_counts(&g)
            .into_iter()
            .filter(|p| p.1 > 0.0)
            .collect();
        assert!(pairs.len() >= 2, "{state:?}: the bars are read: {pairs:?}");
        let unit = pairs[0].0 / pairs[0].1;
        for (height, count) in &pairs {
            assert!(
                (height / count - unit).abs() < 0.5,
                "{state:?}: a bar of {height} px reads Count={count}: {pairs:?}"
            );
        }
    }
}

#[test]
fn legend_toggles_keep_exact_histogram_bin_membership() {
    let chart = chart_of(json!({
        "type": "histogram", "title": "T",
        "data": {"inline": [
            {"g": "A", "v": 1}, {"g": "A", "v": 2}, {"g": "A", "v": 9.996},
            {"g": "B", "v": 11}, {"g": "B", "v": 12}, {"g": "B", "v": 25}
        ]},
        "encoding": {"x": "v", "color": "g"}, "binning": {"edges": [0, 10, 20, 30]}
    }));
    for width in [360.0, 640.0] {
        for (hidden, expected_counts, summary) in [
            (
                vec!["B".into()],
                [3.0, 0.0, 0.0],
                "3 observations from 0 to 30; the fullest interval, 0 to 10, holds 3.",
            ),
            (
                vec!["A".into()],
                [0.0, 2.0, 1.0],
                "3 observations from 0 to 30; the fullest interval, 10 to 20, holds 2.",
            ),
            (
                vec!["A".into(), "B".into()],
                [0.0, 0.0, 0.0],
                "No observations.",
            ),
            (
                vec![],
                [3.0, 2.0, 1.0],
                "6 observations from 0 to 30; the fullest interval, 0 to 10, holds 3.",
            ),
        ] {
            let g = chart.geometry(width, &hidden, None);
            assert_clean("hidden histogram", &g);
            let counts: Vec<_> = heights_and_counts(&g).into_iter().map(|p| p.1).collect();
            assert_eq!(
                counts, expected_counts,
                "{width}, hidden {hidden:?}: the near-edge value stays in its exact bin"
            );
            assert_eq!(g.summary, summary, "{width}, hidden {hidden:?}");
            assert_eq!(
                chart.accessible_name(&g),
                format!("T. {}", summary.trim_end_matches('.')),
                "{width}, hidden {hidden:?}: the accessible name reads the visible tally"
            );
        }
    }
}

#[test]
fn a_filtered_pareto_keeps_exact_near_tie_order_and_running_shares() {
    let mut chart = chart_of(json!({
        "type": "pareto", "title": "T", "encoding": {"x": "c", "y": "n"},
        "data": {"inline": [
            {"r": "North", "c": "Alpha", "n": 100000}, {"r": "North", "c": "Beta", "n": 50000},
            {"r": "North", "c": "Gamma", "n": 20},
            {"r": "South", "c": "Alpha", "n": 1004}, {"r": "South", "c": "Beta", "n": 1006},
            {"r": "South", "c": "Gamma", "n": 20}
        ]},
        "controls": [{"kind": "filter", "id": "r", "field": "r", "options": ["North", "South"]}]
    }));
    chart.set_state(vec![1]);
    for width in [360.0, 640.0] {
        let g = chart.geometry(width, &[], None);
        assert_clean("near-tie Pareto", &g);
        let observations = &g.panels[0].observations;
        let headings: Vec<_> = observations
            .iter()
            .map(|o| o.readout.heading.as_str())
            .collect();
        assert_eq!(headings, ["Beta", "Alpha", "Gamma"]);
        let cumulative: Vec<_> = observations
            .iter()
            .map(|o| {
                o.readout
                    .lines
                    .iter()
                    .find(|l| l.name == "Cumulative share")
                    .unwrap()
                    .value
                    .as_str()
            })
            .collect();
        assert_eq!(cumulative, ["49.6%", "99%", "100%"]);
    }
}

#[test]
fn density_zoom_uses_the_estimate_grid_even_with_native_integer_source_positions() {
    let chart = chart_of(json!({
        "type": "density", "title": "Generated density positions", "encoding": {"x": "v"},
        "style": {"bandwidth": 0.5},
        "data": {"inline": [{"v": 1}, {"v": 2}, {"v": 3}, {"v": 4}, {"v": 5}]}
    }));
    assert_eq!(chart.payload.quant["v"].resolution, Some(1.0));
    let record = chart
        .payload
        .derived
        .iter()
        .flat_map(|d| &d.derived)
        .find(|d| d.panel == 0 && d.key == "density.estimates")
        .unwrap();
    let grid: richview_chart::payload::Quant =
        serde_json::from_value(record.value["x_grid"].clone()).unwrap();
    assert!(grid.steps > 0);
    assert_eq!(grid.resolution, None);
    let step = (grid.max - grid.min) / f64::from(grid.steps);
    for width in [390.0, 640.0] {
        let g = chart.geometry(width, &[], None);
        let scale = g.panels[0].x.as_ref().unwrap();
        let center = scale.extent[0] / 2.0 + scale.extent[1] / 2.0;
        let half = (scale.extent[1] - scale.extent[0]) * 0.00005;
        let window = chart
            .clamp_zoom([center - half, center + half], scale)
            .unwrap();
        let pixels = step / (window[1] - window[0]) * (scale.range[1] - scale.range[0]).abs();
        assert!(
            (pixels - 2.0).abs() < 1e-8,
            "{width}: {pixels} pixels per estimate step"
        );
        assert!(window[0] >= scale.extent[0] && window[1] <= scale.extent[1]);
        assert_eq!(chart.clamp_zoom(window, scale), Some(window));
    }
}

#[test]
fn theoretical_qq_zoom_stops_at_the_declared_derived_grid_not_the_integer_samples() {
    let chart = chart_of(json!({
        "type": "qq", "title": "Theoretical positions", "encoding": {"y": "v"},
        "data": {"inline": [{"v": 1}, {"v": 2}, {"v": 3}, {"v": 4}, {"v": 2000}]}
    }));
    assert!(chart.block.encoding.x.is_none());
    assert_eq!(chart.payload.quant["v"].resolution, Some(1.0));
    let record = chart
        .payload
        .derived
        .iter()
        .flat_map(|d| &d.derived)
        .find(|d| d.panel == 0 && d.key == "qq.ranking")
        .unwrap();
    let grid: richview_chart::payload::Quant =
        serde_json::from_value(record.value["x_grid"].clone()).unwrap();
    assert!(grid.steps > 0);
    assert_eq!(
        grid.resolution, None,
        "theoretical positions are not the integer sample's own resolution"
    );
    let step = (grid.max - grid.min) / f64::from(grid.steps);
    for width in [390.0, 640.0] {
        let geometry = chart.geometry(width, &[], None);
        let scale = geometry.panels[0].x.as_ref().unwrap();
        let center = scale.extent[0] / 2.0 + scale.extent[1] / 2.0;
        let half = (scale.extent[1] - scale.extent[0]) * 0.00005;
        let window = chart
            .clamp_zoom([center - half, center + half], scale)
            .unwrap();
        let pixels = step / (window[1] - window[0]) * (scale.range[1] - scale.range[0]).abs();
        assert!(
            (pixels - 2.0).abs() < 1e-8,
            "{width}: {pixels} pixels per theoretical step"
        );
        assert!(window[0] >= scale.extent[0] && window[1] <= scale.extent[1]);
        assert_eq!(chart.clamp_zoom(window, scale), Some(window));
    }
}

#[test]
fn zoom_stops_at_two_pixels_per_grid_step_and_keeps_the_window_inside_the_data() {
    let chart = chart_of(json!({
        "type": "line", "title": "T", "encoding": {"x": "t", "y": "v"},
        "data": {"inline": [{"t": 0.0, "v": 1}, {"t": 0.1234, "v": 2}, {"t": 10.0, "v": 3}]}
    }));
    let q = &chart.payload.quant["t"];
    let step = (q.max - q.min) / f64::from(q.steps);
    for width in [360.0, 1100.0] {
        let g = chart.geometry(width, &[], None);
        let s = g.panels[0].x.as_ref().unwrap();
        for center in [s.extent[0], 5.0, s.extent[1]] {
            let z = chart.clamp_zoom([center - 1e-9, center + 1e-9], s).unwrap();
            assert!(z[0] >= s.extent[0] && z[1] <= s.extent[1]);
            let pixels = step / (z[1] - z[0]) * (s.range[1] - s.range[0]).abs();
            assert!(
                (pixels - 2.0).abs() < 1e-8,
                "{width}: {z:?}, {pixels} pixels per step"
            );
            assert_eq!(
                chart.clamp_zoom(z, s),
                Some(z),
                "repeated zooms stay at the floor"
            );
        }
        assert_eq!(
            chart.clamp_zoom(s.extent, s),
            None,
            "the whole chart resets"
        );
        assert_eq!(chart.clamp_zoom([f64::NAN, 1.0], s), None);
    }
}

#[test]
fn integer_zoom_requires_explicit_native_resolution_not_just_a_unit_grid_step() {
    let chart = chart_of(json!({
        "type": "line", "title": "Integer positions", "encoding": {"x": "t", "y": "v"},
        "data": {"inline": [{"t": 0, "v": 1}, {"t": 2000, "v": 2}, {"t": 4000, "v": 3}]}
    }));
    let q = &chart.payload.quant["t"];
    assert_eq!(q.resolution, Some(1.0));
    let step = (q.max - q.min) / f64::from(q.steps);
    assert_eq!(step, 1.0);

    // The optional marker may be absent in serialized payloads. A unit step alone never
    // proves the positions are native integers: it can also be a lossy decimal grid.
    let mut json = serde_json::to_value(&chart.payload).unwrap();
    json["quant"]["t"]
        .as_object_mut()
        .unwrap()
        .remove("resolution");
    let unknown = Chart::parse(&json.to_string()).unwrap();
    assert_eq!(unknown.payload.quant["t"].resolution, None);
    let requested = [1999.9, 2000.1];
    for width in [360.0, 1100.0] {
        let whole = chart.geometry(width, &[], None);
        let scale = whole.panels[0].x.as_ref().unwrap();
        assert_eq!(
            chart.clamp_zoom(requested, scale),
            Some(requested),
            "native integers retain their own zoom bound"
        );
        let zoomed = chart.geometry(width, &[], Some(requested));
        assert_eq!(zoomed.panels[0].observations.len(), 1);
        let observation = &zoomed.panels[0].observations[0];
        assert_eq!(observation.readout.heading, "T 2,000");
        assert_eq!(observation.readout.lines[0].value, "2");

        let limited = unknown.clamp_zoom(requested, scale).unwrap();
        let pixels = step / (limited[1] - limited[0]) * (scale.range[1] - scale.range[0]).abs();
        assert!(
            (pixels - 2.0).abs() < 1e-8,
            "a unit step with no native marker still stops at two pixels: {pixels}"
        );
    }
}

#[test]
fn zoomed_line_readouts_follow_the_visible_points_by_identity() {
    let rows: Vec<_> = (0..40).map(|i| json!({"t": i, "v": 3 + i * 7})).collect();
    let chart = chart_of(json!({
        "type": "line", "title": "T", "encoding": {"x": "t", "y": "v"},
        "data": {"inline": rows}
    }));
    for width in [360.0, 640.0] {
        let g = chart.geometry(width, &[], Some([10.0, 25.0]));
        assert_clean("zoomed line", &g);
        let observations = &g.panels[0].observations;
        assert!(!observations.is_empty());
        for o in observations {
            let t: usize = o
                .readout
                .heading
                .strip_prefix("T ")
                .unwrap()
                .parse()
                .unwrap();
            assert!((10..=25).contains(&t), "{o:?}");
            assert_eq!(o.readout.lines[0].value, (3 + t * 7).to_string());
        }
        assert_eq!(observations[0].readout.heading, "T 10");
    }
}

#[test]
fn accessible_names_follow_the_selected_state_and_its_summary() {
    let mut chart = chart_of(json!({
        "type": "bar", "title": "Revenue", "encoding": {"x": "q", "y": "v"},
        "data": {"inline": rows()},
        "controls": [{"kind": "filter", "id": "r", "field": "region", "options": ["North", "South"]}]
    }));
    let initial = chart.geometry(640.0, &[], None);
    let name = chart.accessible_name(&initial);
    assert!(name.contains("Revenue. North."), "{name}");
    assert!(name.contains(initial.summary.trim_end_matches('.')));
    chart.set_state(vec![1]);
    let filtered = chart.geometry(640.0, &[], None);
    let name = chart.accessible_name(&filtered);
    assert!(name.contains("Revenue. South."), "{name}");
    assert!(name.contains(filtered.summary.trim_end_matches('.')));
    assert!(!name.contains("North"));
}

#[test]
fn the_monthly_flagship_keeps_its_calendar_zoom_and_month_labels() {
    let report = include_str!("../../../examples/openai-vs-anthropic/report.rmd");
    let block = report
        .split("```chart\n")
        .skip(1)
        .find(|block| block.starts_with("type: line\n"))
        .unwrap()
        .split("\n```")
        .next()
        .unwrap();
    let chart = chart_from_source(&format!("```chart\n{block}\n```\n"));
    let whole = chart.geometry(640.0, &[], None);
    let scale = whole.panels[0].x.as_ref().unwrap();
    assert!(scale.calendar);
    let day = richview_chart::time::MS_DAY as f64;
    let center = scale.domain[0] / 2.0 + scale.domain[1] / 2.0;
    let requested = [center - 45.0 * day, center + 45.0 * day];
    let zoom = chart
        .clamp_zoom(requested, scale)
        .expect("monthly dates still zoom");
    assert_eq!(
        zoom, requested,
        "calendar values keep their existing zoom bound"
    );
    let g = chart.geometry(640.0, &[], Some(zoom));
    assert!(g.panels[0].observations.len() < whole.panels[0].observations.len());
    assert!(!g.panels[0].observations.is_empty());
    for observation in &g.panels[0].observations {
        let heading = &observation.readout.heading;
        assert_eq!(
            heading.split_whitespace().count(),
            2,
            "monthly readouts stay at month/year resolution: {heading}"
        );
    }
}
#[test]
fn density_interactions_read_the_exact_recorded_anchors_after_core_transport() {
    let mut rows = vec![
        json!({"v":100.01,"g":"close"}),
        json!({"v":100.04,"g":"close"}),
    ];
    rows.extend([0, 250, 500, 750, 1000].map(|v| json!({"v":v,"g":"wide"})));
    let chart = chart_of(json!({
        "type":"density", "title":"Narrow and wide", "digits":6,
        "encoding":{"x":"v","color":"g"}, "data":{"inline":rows}
    }));
    let record = &chart
        .payload
        .derived
        .iter()
        .flat_map(|d| &d.derived)
        .find(|d| d.panel == 0 && d.key == "density.estimates")
        .unwrap()
        .value;
    let grid: richview_chart::payload::Quant =
        serde_json::from_value(record["x_grid"].clone()).unwrap();
    let anchors = record["panels"][0]["readouts"].as_array().unwrap();
    assert!(anchors.len() > 17 && anchors.len() <= 257);
    assert_eq!(anchors.iter().filter(|a| a["full"] == true).count(), 17);
    for width in [390.0, 640.0] {
        for (hidden, zoom) in [
            (vec![], None),
            (vec![], Some([100.0, 100.05])),
            (vec![], Some([250.0, 500.0])),
            (vec!["wide".to_owned()], Some([100.0, 100.05])),
            (vec!["wide".to_owned()], None),
            (vec![], None),
        ] {
            let g = chart.geometry(width, &hidden, zoom);
            assert!(
                g.failure.is_none(),
                "{width}/{hidden:?}/{zoom:?}: {:?}",
                g.failure
            );
            let panel = &g.panels[0];
            let scale = panel.x.as_ref().unwrap();
            let expected: Vec<_> = anchors
                .iter()
                .filter_map(|a| {
                    if zoom.is_none() && a["full"] != true {
                        return None;
                    }
                    let index = a["x"]["grid"]
                        .as_u64()
                        .expect("generated x is on its declared grid");
                    let x = grid.min + (grid.max - grid.min) * index as f64 / f64::from(grid.steps);
                    if x < scale.domain[0] || x > scale.domain[1] {
                        return None;
                    }
                    let lines: Vec<_> = a["values"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .enumerate()
                        .filter_map(|(i, v)| {
                            let name = ["close", "wide"][i];
                            if hidden.iter().any(|h| h == name) || v.is_null() {
                                return None;
                            }
                            Some((
                                format!("{name} · Density"),
                                v[1].as_str().unwrap().to_owned(),
                            ))
                        })
                        .collect();
                    if lines.is_empty() {
                        return None;
                    }
                    let px = scale.range[0]
                        + (x - scale.domain[0]) / (scale.domain[1] - scale.domain[0])
                            * (scale.range[1] - scale.range[0]);
                    Some((px, a["heading"].as_str().unwrap(), lines))
                })
                .collect();
            assert_eq!(
                panel.observations.len(),
                expected.len(),
                "{width}/{hidden:?}/{zoom:?}"
            );
            for (o, (x, heading, lines)) in panel.observations.iter().zip(&expected) {
                assert!(
                    (o.x - x).abs() < 1e-7,
                    "recorded anchor remains at its own x"
                );
                assert_eq!(
                    o.readout.heading, *heading,
                    "{width}/{hidden:?}/{zoom:?}: heading"
                );
                assert_eq!(
                    o.readout
                        .lines
                        .iter()
                        .map(|l| (l.name.clone(), l.value.clone()))
                        .collect::<Vec<_>>(),
                    *lines,
                    "{width}/{hidden:?}/{zoom:?}: exact density strings"
                );
            }
            if zoom == Some([100.0, 100.05]) {
                assert!(
                    panel.observations.iter().any(|o| o
                        .readout
                        .lines
                        .iter()
                        .any(|l| l.name == "close · Density")),
                    "the finer anchors keep the narrow curve readable"
                );
            }
        }
    }
}

#[test]
fn hidden_cartesian_ranges_replay_shown_values_for_axes_and_marks() {
    for kind in ["line", "area", "scatter", "bar"] {
        let chart = chart_of(json!({
            "type":kind,"title":"T","digits":3,"encoding":{"x":"x","y":"y","color":"g"},
            "data":{"inline":[
                {"x":0,"y":10.049,"g":"Low"},{"x":1,"y":10.151,"g":"Low"},
                {"x":0,"y":1000.049,"g":"High"},{"x":1,"y":1259.151,"g":"High"}
            ]}
        }));
        let whole = chart.geometry(640.0, &[], None);
        for width in [360.0, 640.0] {
            for zoom in [None, Some([0.0, 0.5])] {
                let hidden = chart.geometry(width, &["High".into()], zoom);
                assert_eq!(
                    hidden.fit.y,
                    Some([10.0, 10.2]),
                    "{kind}, {width}px, zoom {zoom:?}: shown subset sets both layouts' range"
                );
                assert!(
                    texts(&hidden)
                        .iter()
                        .all(|v| v != "10.049" && v != "10.151"),
                    "readers get the chosen precision"
                );
                assert_eq!(
                    chart.geometry(width, &[], zoom).fit.y,
                    whole.fit.y,
                    "showing the series restores its range"
                );
            }
        }
    }
}
