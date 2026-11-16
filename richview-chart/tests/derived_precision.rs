//! Derived geometry preserves source membership and exposes only the chosen precision (TYP-36).

mod common;

use common::{draw, line, observations};
use richview_chart::numbers::{self, Format};
use serde_json::{Value, json};

#[test]
fn measured_channels_declare_their_shared_scales() {
    use richview_chart::block::ChartBlock;
    for (kind, encoding, expected) in [
        (
            "interval",
            json!({"x":"name","y":"estimate","lower":"lo","upper":"hi","detail":["estimate","note"]}),
            vec![("estimate", "value"), ("lo", "value"), ("hi", "value")],
        ),
        (
            "candlestick",
            json!({"x":"date","open":"o","high":"h","low":"l","close":"c"}),
            vec![("o", "value"), ("h", "value"), ("l", "value"), ("c", "value")],
        ),
        (
            "bullet",
            json!({"x":"name","y":"actual","target":"target"}),
            vec![("actual", "value"), ("target", "value")],
        ),
        (
            "gantt",
            json!({"label":"name","start":"s","end":"e"}),
            vec![("s", "time"), ("e", "time")],
        ),
        (
            "parallel",
            json!({"dimensions":["a","b"]}),
            vec![("a", "dimension:a"), ("b", "dimension:b")],
        ),
        (
            "scatter",
            json!({"x":"x","y":"y","size":"s","detail":["note"]}),
            vec![("x", "x"), ("y", "value"), ("s", "size")],
        ),
    ] {
        let block = ChartBlock::from_value(&json!({"type":kind,"title":"T","encoding":encoding})).unwrap();
        for (field, scale) in expected {
            assert_eq!(block.scale_of(field).as_deref(), Some(scale), "{kind}.{field}");
        }
        assert_eq!(block.scale_of("note"), None);
        assert_eq!(block.scale_of("missing"), None);
    }
}

#[test]
fn authored_histogram_edges_and_membership_stay_exact() {
    let spec = json!({"type":"histogram","title":"T","encoding":{"x":"v"},"binning":{"edges":[0,1.255,2.51]}});
    let rows: Vec<Value> = [0.2, 1.254, 1.258, 2.0, 2.5]
        .into_iter()
        .map(|v| json!({"v":v}))
        .collect();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    assert_eq!(
        obs.iter().map(|o| line(o, "Count").unwrap()).collect::<Vec<_>>(),
        ["2", "3"]
    );
    assert!(obs[0].readout.heading.contains("1.255"), "{}", obs[0].readout.heading);
    assert!(obs[1].readout.heading.contains("2.51"), "{}", obs[1].readout.heading);
}

#[test]
fn widened_automatic_bins_keep_the_authored_extent_filter() {
    let spec = json!({"type":"histogram","title":"T","encoding":{"x":"v"},
        "binning":{"count":2,"extent":[1.234,2.345]}});
    let rows: Vec<Value> = [1.232, 1.235, 2.34, 2.346]
        .into_iter()
        .map(|v| json!({"v":v}))
        .collect();
    let g = draw(&spec, &rows);
    let counts: usize = observations(&g)
        .iter()
        .map(|o| line(o, "Count").unwrap().parse::<usize>().unwrap())
        .sum();
    assert_eq!(
        counts, 2,
        "nice outer edges must not admit rows outside the authored extent"
    );
}

#[test]
fn authored_extent_keeps_its_bounds_and_density_catchment() {
    let spec = json!({"type":"histogram","title":"T","encoding":{"x":"v"},
        "binning":{"count":7,"extent":[0,1],"measure":"density"}});
    let rows = vec![json!({"v":0.95}), json!({"v":1.02})];
    let g = draw(&spec, &rows);
    let edges = &g.derived.iter().find(|d| d.key == "histogram.edges").unwrap().value["Ok"];
    assert_eq!(edges.as_array().unwrap().last().unwrap(), &json!(1.0));
    let last = observations(&g).last().copied().unwrap();
    let width = edges[edges.as_array().unwrap().len() - 1].as_f64().unwrap()
        - edges[edges.as_array().unwrap().len() - 2].as_f64().unwrap();
    let density = line(last, "Density").unwrap().parse::<f64>().unwrap();
    assert!((density * width - 1.0).abs() < 0.001);
    assert!(last.readout.heading.ends_with(" to 1 (inclusive)"));
    assert!(common::check(&g, "binning").unwrap().message.contains("requested 7"));
}

#[test]
fn high_precision_edges_keep_the_chosen_bin_membership() {
    let spec = json!({"type":"histogram","title":"T","digits":10,"encoding":{"x":"v"},
        "binning":{"count":2}});
    let rows = vec![
        json!({"v":1.23456789}),
        json!({"v":1.23456801}),
        json!({"v":1.23456813}),
    ];
    let g = draw(&spec, &rows);
    let record = &g.derived.iter().find(|d| d.key == "histogram.edges").unwrap().value["Ok"];
    let edges: Vec<f64> = record.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!(edges.windows(2).all(|w| w[0] < w[1]));
    let counts: usize = observations(&g)
        .iter()
        .map(|o| line(o, "Count").unwrap().parse::<usize>().unwrap())
        .sum();
    assert_eq!(counts, rows.len());
    assert!(edges[0] <= 1.23456789 && edges[0] > 1.23456788);
}

#[test]
fn automatic_bin_edges_are_chosen_at_the_shown_precision() {
    let values = [1.23456789, 2.3456789, 3.456789, 4.5678912, 9.87654321];
    for (kind, encoding, key) in [
        ("histogram", json!({"x":"v"}), "histogram.edges"),
        ("dotplot", json!({"x":"v"}), "dotplot.edges"),
        ("bin2d", json!({"x":"v","y":"w"}), "bin2d.edges"),
    ] {
        let spec = json!({"type":kind,"title":"T","encoding":encoding});
        let rows: Vec<Value> = values
            .iter()
            .enumerate()
            .map(|(i, v)| json!({"v":v,"w":values[values.len()-1-i]}))
            .collect();
        let g = draw(&spec, &rows);
        assert!(g.failure.is_none(), "{kind}: {:?}", g.failure);
        let edges = &g.derived.iter().find(|d| d.key == key).unwrap().value["Ok"];
        fn check(value: &Value) {
            match value {
                Value::Number(n) => {
                    let n = n.as_f64().unwrap();
                    assert_eq!(
                        n,
                        numbers::shown(n, &Format::default()),
                        "edge {n} exceeds shown precision"
                    );
                }
                Value::Array(values) => values.iter().for_each(check),
                _ => panic!("unexpected edges: {value}"),
            }
        }
        check(edges);
    }
}

#[test]
fn narrow_boxplots_keep_geometry_and_formatter_precision() {
    let spec = json!({"type":"boxplot","title":"T","encoding":{"x":"g","y":"v"}});
    let rows: Vec<Value> = [
        ("North", 1001.3),
        ("North", 1002.7),
        ("North", 1003.1),
        ("North", 1004.1),
        ("North", 1005.5),
        ("North", 1006.2),
        ("North", 1008.9),
        ("South", 1003.4),
        ("South", 1004.9),
        ("South", 1005.1),
        ("South", 1006.6),
        ("South", 1007.2),
        ("South", 1007.8),
        ("South", 1009.4),
    ]
    .into_iter()
    .map(|(g, v)| json!({"g":g,"v":v}))
    .collect();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    assert_eq!(line(obs[0], "Median"), Some("1,004"));
    assert_eq!(line(obs[1], "Median"), Some("1,007"));
    assert_eq!(line(obs[0], "Lower whisker"), Some("1,001"));
    assert_eq!(line(obs[1], "Lower whisker"), Some("1,003"));
    let boxes: Vec<_> = common::marks(&g)
        .into_iter()
        .filter_map(|item| match item {
            richview_chart::geometry::Item::Rect { rect, .. } => Some(rect),
            _ => None,
        })
        .collect();
    assert_eq!(boxes.len(), 2);
    assert!(boxes.iter().all(|r| r.h > 1.0 && r.h < g.panels[0].plot.h * 0.8));
}

#[test]
fn constant_boxplot_uses_only_shown_coordinates() {
    let spec = json!({"type":"boxplot","title":"T","encoding":{"x":"g","y":"v"}});
    let rows = vec![json!({"g":"A","v":1.23456789}); 4];
    let g = draw(&spec, &rows);
    let record = &g.derived.iter().find(|d| d.key == "boxplot.summaries").unwrap().value;
    assert!(record["grid"].is_null());
    for field in ["min", "q1", "median", "q3", "max", "lower_whisker", "upper_whisker"] {
        assert_eq!(record["groups"][0][field], json!({"shown":1.23}));
    }
    assert_eq!(line(observations(&g)[0], "Median"), Some("1.23"));
}

#[test]
fn fractional_box_statistics_refine_only_the_record_grid() {
    use richview_chart::{
        block::ChartBlock,
        layout::{ChartTheme, Options, layout},
        payload::Quant,
    };
    let block = ChartBlock::from_value(&json!({"type":"boxplot","title":"T","encoding":{"x":"g","y":"v"}})).unwrap();
    let rows: Vec<_> = (1..=4).map(|v| json!({"g":"A","v":v})).collect();
    for steps in [3, 10000] {
        let raw = Quant {
            steps,
            ..Quant::over([1.0, 4.0]).unwrap()
        };
        let opts = Options {
            width: 640.0,
            grids: std::collections::BTreeMap::from([("v".into(), raw.clone())]),
            ..Default::default()
        };
        let g = layout(&block, &rows, &ChartTheme::default(), &opts);
        let record = &g.derived.iter().find(|d| d.key == "boxplot.summaries").unwrap().value;
        let grid: Quant = serde_json::from_value(record["grid"].clone()).unwrap();
        assert_eq!((grid.min, grid.max), (raw.min, raw.max));
        assert_eq!(grid.steps, steps.max(4095));
        assert_eq!(opts.grids["v"], raw, "the observation grid stays untouched");
        for (field, want) in [("q1", 1.75), ("median", 2.5), ("q3", 3.25)] {
            let index = record["groups"][0][field]["grid"].as_u64().unwrap();
            assert!((grid.at(index) - want).abs() <= (grid.max - grid.min) / f64::from(grid.steps) / 2.0 + 1e-12);
        }
    }
}

#[test]
fn packed_bubble_sizes_do_not_recover_exact_source_values() {
    let spec = json!({"type":"packed-bubble","title":"T","encoding":{"label":"name","y":"v"}});
    let rows = vec![
        json!({"name":"A","v":123.456789}),
        json!({"name":"B","v":98.7654321}),
        json!({"name":"C","v":45.6789123}),
    ];
    let g = draw(&spec, &rows);
    let packing = &g
        .derived
        .iter()
        .find(|d| d.key == "packed-bubble.packing")
        .unwrap()
        .value;
    for spot in packing.as_array().unwrap() {
        let row = spot[0].as_u64().unwrap() as usize;
        let size = spot[2].as_f64().unwrap();
        let shown = numbers::shown(rows[row]["v"].as_f64().unwrap(), &Format::default());
        assert_eq!(
            size, shown,
            "packing stores the shown size; geometry takes its square root"
        );
    }
}

#[test]
fn packed_bubbles_with_equal_shown_sizes_keep_source_order() {
    let spec = json!({"type":"packed-bubble","title":"T","encoding":{"label":"name","y":"v"}});
    let rows = vec![json!({"name":"A","v":12.31}), json!({"name":"B","v":12.34})];
    let g = draw(&spec, &rows);
    let packing = &g
        .derived
        .iter()
        .find(|d| d.key == "packed-bubble.packing")
        .unwrap()
        .value;
    assert_eq!(packing[0][0], json!(0));
    assert_eq!(packing[0][1], json!([0.0, 0.0]));
}

#[test]
fn wordcloud_sizes_use_a_declared_float_grid_and_shown_value_ties() {
    use richview_chart::payload::Quant;
    let spec = json!({"type":"wordcloud","title":"T","encoding":{"label":"name","y":"v"}});
    let rows = vec![
        json!({"name":"A","v":12.31}),
        json!({"name":"B","v":12.34}),
        json!({"name":"C","v":10.0}),
    ];
    let g = draw(&spec, &rows);
    let plan = &g.derived.iter().find(|d| d.key == "wordcloud.plan").unwrap().value;
    let grid: Quant = serde_json::from_value(plan["grid"].clone()).expect("word sizes carry their declared grid");
    assert_eq!((grid.min, grid.max), (0.0, 1.0));
    assert!(grid.steps >= 4095);
    assert_eq!(grid.resolution, None, "derived sizes have no native integer resolution");
    for value in plan["fraction"].as_array().unwrap() {
        let index = value.as_u64().expect("a size is a grid index");
        assert!(index <= u64::from(grid.steps));
        assert!((0.0..=1.0).contains(&grid.at(index)));
    }
    assert_eq!(plan["order"][0], json!(0), "equal shown sizes keep source order");
}

#[test]
fn treemap_edges_use_a_declared_shared_grid_at_the_authors_precision() {
    use richview_chart::payload::Quant;
    for digits in [3, 7] {
        let spec = json!({"type":"treemap","title":"T","digits":digits,
            "encoding":{"label":"name","parent":"parent","y":"v"}});
        let rows = vec![
            json!({"name":"A","v":123.456789}),
            json!({"name":"B","v":98.7654321}),
            json!({"name":"C","v":45.6789123}),
        ];
        let g = draw(&spec, &rows);
        let plan = &g.derived.iter().find(|d| d.key == "treemap.tiles").unwrap().value;
        let grid: Quant = serde_json::from_value(plan["grid"].clone()).expect("tile edges declare their grid");
        assert_eq!((grid.min, grid.max), (0.0, 1.0));
        assert!(grid.steps >= 4095);
        assert_eq!(grid.resolution, None);
        if digits == 7 {
            assert!(
                grid.steps > 4095,
                "preserve positions the finer shown precision distinguishes"
            );
        }
        let tiles = plan["tiles"].as_array().unwrap();
        let plot = g.panels[0].plot;
        let total: f64 = rows.iter().map(|r| r["v"].as_f64().unwrap()).sum();
        for (tile, o) in tiles.iter().zip(observations(&g)) {
            let indices: [u64; 4] = tile["bounds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().expect("each edge is an integer grid index"))
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            assert!(indices.iter().all(|k| *k <= u64::from(grid.steps)));
            let [left, top, right, bottom] = indices.map(|k| grid.at(k));
            let rect = o.area.unwrap();
            assert_eq!(rect.x, plot.x + left * plot.w);
            assert_eq!(rect.y, plot.y + top * plot.h);
            assert_eq!(rect.w, (right - left) * plot.w);
            assert_eq!(rect.h, (bottom - top) * plot.h);
            let exact = rows[o.row.unwrap()]["v"].as_f64().unwrap() / total;
            let drawn = rect.w * rect.h / (plot.w * plot.h);
            assert!((drawn - exact).abs() <= 1.0 / f64::from(grid.steps));
        }
    }
}

#[test]
fn pie_boundaries_and_label_angles_replay_from_one_declared_grid() {
    use richview_chart::payload::Quant;
    let spec = json!({"type":"pie","title":"T","encoding":{"x":"name","y":"v"}});
    let rows = vec![
        json!({"name":"A","v":1.23456789}),
        json!({"name":"B","v":2.34567891}),
        json!({"name":"C","v":3.45678912}),
    ];
    let g = draw(&spec, &rows);
    let plan = &g.derived.iter().find(|d| d.key == "pie.angles").unwrap().value;
    let grid: Quant = serde_json::from_value(plan["grid"].clone()).expect("pie angles declare their grid");
    assert_eq!((grid.min, grid.max), (0.0, 1.0));
    assert!(grid.steps >= 4095);
    assert_eq!(grid.resolution, None);
    let edges = plan["edges"].as_array().unwrap();
    let mut previous = 0;
    for o in observations(&g) {
        let slot = o.series.unwrap();
        let edge = &edges[slot];
        let lo = edge[0].as_u64().expect("a boundary is a grid index");
        let hi = edge[1].as_u64().expect("a boundary is a grid index");
        assert_eq!(lo, previous, "adjoining wedges share a boundary");
        assert!(lo <= hi && hi <= u64::from(grid.steps));
        let wedge = o.wedge.unwrap();
        assert_eq!(wedge.start, grid.at(lo) * std::f64::consts::TAU);
        assert_eq!(wedge.end, grid.at(hi) * std::f64::consts::TAU);
        previous = hi;
    }
    assert_eq!(previous, u64::from(grid.steps), "the wedges fill the whole disk");
    common::runtime_matches_native(&spec, &rows);
}

#[test]
fn violin_statistics_and_curve_samples_use_declared_float_grids() {
    use richview_chart::payload::Quant;
    let spec = json!({"type":"violin","title":"T","encoding":{"x":"name","y":"v"}});
    let rows: Vec<_> = (1..=4).map(|v| json!({"name":"A","v":v})).collect();
    let g = draw(&spec, &rows);
    let record = &g.derived.iter().find(|d| d.key == "violin.forms").unwrap().value;
    let value: Quant =
        serde_json::from_value(record["value_grid"].clone()).expect("value positions declare their grid");
    let density: Quant =
        serde_json::from_value(record["density_grid"].clone()).expect("density positions declare their grid");
    for grid in [&value, &density] {
        assert!(grid.steps >= 4095);
        assert_eq!(grid.resolution, None, "statistics have no raw integer resolution");
    }
    assert_eq!((value.min, value.max), (1.0, 4.0));
    let form = &record["panels"][0][0];
    for (q, want) in form["quartiles"].as_array().unwrap().iter().zip([1.75, 2.5, 3.25]) {
        let index = q["grid"].as_u64().expect("quartiles are grid indices");
        assert!(index <= u64::from(value.steps));
        assert!((value.at(index) - want).abs() <= (value.max - value.min) / f64::from(value.steps));
    }
    for p in form["curve"].as_array().unwrap() {
        for (coordinate, grid) in p.as_array().unwrap().iter().zip([&value, &density]) {
            assert!(coordinate["grid"].as_u64().unwrap() <= u64::from(grid.steps));
        }
    }
    assert_eq!(line(observations(&g)[0], "Median"), Some("2.5"));
    common::runtime_matches_native(&spec, &rows);
}

#[test]
fn derived_quartile_grids_keep_shown_decimal_resolution_across_integer_groups() {
    use richview_chart::payload::Quant;
    let rows: Vec<_> = [("A", 1), ("B", 1000)]
        .into_iter()
        .flat_map(|(name, unit)| (1..=4).map(move |v| json!({"name":name,"v":v*unit})))
        .collect();
    for kind in ["boxplot", "violin"] {
        let spec = json!({"type":kind,"title":"T","encoding":{"x":"name","y":"v"}});
        let g = draw(&spec, &rows);
        let key = if kind == "boxplot" {
            "boxplot.summaries"
        } else {
            "violin.forms"
        };
        let record = &g.derived.iter().find(|d| d.key == key).unwrap().value;
        let (grid, q1) = if kind == "boxplot" {
            (&record["grid"], &record["groups"][0]["q1"])
        } else {
            (&record["value_grid"], &record["panels"][0][0]["quartiles"][0])
        };
        let grid: Quant = serde_json::from_value(grid.clone()).unwrap();
        assert_eq!((grid.min, grid.max), (1.0, 4000.0));
        assert_eq!(grid.resolution, None);
        assert!(
            (grid.max - grid.min) / f64::from(grid.steps) <= 0.01,
            "{kind}: shown 1.75 distinguishes hundredths"
        );
        assert!((grid.at(q1["grid"].as_u64().unwrap()) - 1.75).abs() <= 0.005);
    }
}

#[test]
fn qq_quantiles_and_quartile_pairs_use_shared_grids_and_separate_text() {
    use richview_chart::payload::Quant;
    let rows: Vec<_> = [("A", 1), ("B", 1000)]
        .into_iter()
        .flat_map(|(group, unit)| (1..=4).map(move |v| json!({"g":group,"v":v*unit})))
        .collect();
    let spec = json!({"type":"qq","title":"T","encoding":{"y":"v","color":"g"}});
    let g = draw(&spec, &rows);
    let record = &g
        .derived
        .iter()
        .find(|d| d.key.starts_with("qq.ranking"))
        .unwrap()
        .value;
    let x: Quant = serde_json::from_value(record["x_grid"].clone()).expect("theoretical positions declare their grid");
    let y: Quant = serde_json::from_value(record["value_grid"].clone()).expect("quartile pairs declare their grid");
    for grid in [&x, &y] {
        assert!(grid.steps >= 4095);
        assert_eq!(grid.resolution, None);
    }
    assert_eq!((y.min, y.max), (1.0, 4000.0));
    assert!((y.max - y.min) / f64::from(y.steps) <= 0.01);
    let small = &record["panels"][0][0];
    for (point, want) in small["line"].as_array().unwrap().iter().zip([1.75, 3.25]) {
        for (coordinate, grid) in point.as_array().unwrap().iter().zip([&x, &y]) {
            assert!(coordinate["grid"].as_u64().unwrap() <= u64::from(grid.steps));
        }
        assert!((y.at(point[1]["grid"].as_u64().unwrap()) - want).abs() <= 0.005);
    }
    let expected = ["−1.15", "−0.319", "0.319", "1.15"];
    assert_eq!(small["z_texts"], json!(expected));
    for (o, text) in observations(&g).iter().take(4).zip(expected) {
        assert_eq!(line(o, "Theoretical quantile"), Some(text));
    }
    let scale = g.panels[0].x.as_ref().unwrap();
    for (o, z) in observations(&g).iter().take(4).zip(small["z"].as_array().unwrap()) {
        let z = x.at(z["grid"].as_u64().unwrap());
        let pixel = scale.range[0]
            + (z - scale.domain[0]) / (scale.domain[1] - scale.domain[0]) * (scale.range[1] - scale.range[0]);
        assert!((o.at[0] - pixel).abs() < 1e-10, "the point uses its recorded position");
    }
    for ranking in record["panels"][0].as_array().unwrap() {
        assert_eq!(ranking["z"].as_array().unwrap().len(), 4);
        assert!(
            ranking["z"]
                .as_array()
                .unwrap()
                .iter()
                .all(|z| z["grid"].as_u64().unwrap() <= u64::from(x.steps))
        );
    }
    common::runtime_matches_native(&spec, &rows);
}

#[test]
fn venn_areas_and_overlap_centres_use_one_grid_across_facets() {
    use richview_chart::payload::Quant;
    let spec = json!({"type":"venn","title":"T","encoding":{"sets":"s","y":"v","facet":"f"}});
    let rows: Vec<_> = [("first", 123.456789, 45.6789123), ("second", 30.86419725, 12.3456789)]
        .into_iter()
        .flat_map(|(f, a, overlap)| {
            [("A", a), ("B", 98.7654321), ("A&B", overlap)]
                .into_iter()
                .map(move |(s, v)| json!({"s":s,"v":v,"f":f}))
        })
        .collect();
    let g = draw(&spec, &rows);
    let record = &g.derived.iter().find(|d| d.key == "venn.solved").unwrap().value;
    let area: Quant = serde_json::from_value(record["area_grid"].clone()).expect("set areas declare their shared grid");
    let position: Quant =
        serde_json::from_value(record["position_grid"].clone()).expect("overlap centres declare their shared grid");
    assert_eq!((area.min, area.max), (0.0, 1.0));
    for grid in [&area, &position] {
        assert!(grid.steps >= 4095);
        assert_eq!(grid.resolution, None);
    }
    let panels = record["panels"].as_array().unwrap();
    assert_eq!(panels.len(), 2);
    assert_eq!(
        panels[0]["areas"][1], panels[1]["areas"][1],
        "equal sets across facets keep equal areas"
    );
    let largest_radius = observations(&g)
        .into_iter()
        .filter(|o| o.series.is_some())
        .map(|o| o.area.unwrap().w / 2.0)
        .fold(0.0f64, f64::max);
    for ((panel, geometry), want) in panels
        .iter()
        .zip(&g.panels)
        .zip([[1.0, 98.7654321 / 123.456789], [0.25, 98.7654321 / 123.456789]])
    {
        for (index, fraction) in panel["areas"].as_array().unwrap().iter().zip(want) {
            let index = index["grid"].as_u64().expect("a set area is a grid index");
            assert!(index <= u64::from(area.steps));
            assert!((area.at(index) - fraction).abs() <= 1.0 / f64::from(area.steps));
        }
        for centre in panel["centres"].as_array().unwrap() {
            for coordinate in centre.as_array().unwrap() {
                assert!(coordinate["grid"].as_u64().unwrap() <= u64::from(position.steps));
            }
        }
        let circles: Vec<_> = geometry
            .observations
            .iter()
            .filter(|o| o.series.is_some())
            .map(|o| o.area.unwrap())
            .collect();
        for (k, circle) in circles.iter().enumerate() {
            let fraction = area.at(panel["areas"][k]["grid"].as_u64().unwrap());
            assert!(((circle.w / (2.0 * largest_radius)).powi(2) - fraction).abs() < 1e-12);
            for axis in 0..2 {
                let decoded = |k: usize| position.at(panel["centres"][k][axis]["grid"].as_u64().unwrap());
                let expected = (decoded(k) - decoded(0)) * largest_radius;
                assert!((circle.center()[axis] - circles[0].center()[axis] - expected).abs() < 1e-9);
            }
        }
    }
    common::runtime_matches_native(&spec, &rows);
    let mut coarse = spec;
    coarse["digits"] = json!(1);
    let g = draw(&coarse, &rows);
    let pair = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading == "A and B")
        .unwrap();
    let format = Format {
        kind: richview_chart::numbers::Kind::Percent,
        digits: 1,
        ..Format::default()
    };
    assert_eq!(
        line(pair, "Share of A"),
        Some(numbers::value(45.6789123 / 123.456789, &format).as_str())
    );
}

#[test]
fn density_curves_and_exact_readouts_share_declared_padded_grids() {
    use richview_chart::payload::Quant;
    let spec = json!({"type":"density","title":"T","digits":6,"encoding":{"x":"v"},"style":{"bandwidth":0.5}});
    let rows = vec![json!({"v":0}), json!({"v":1})];
    let g = draw(&spec, &rows);
    let record = &g.derived.iter().find(|d| d.key.starts_with("density.")).unwrap().value;
    let x: Quant =
        serde_json::from_value(record["x_grid"].clone()).expect("density positions declare their padded grid");
    let y: Quant = serde_json::from_value(record["density_grid"].clone()).expect("densities declare their shared grid");
    assert!(x.min <= -1.5 && x.max >= 2.5, "retain Gaussian tails");
    let step = (x.max - x.min) / f64::from(x.steps);
    assert!(
        step <= 1e-6,
        "derived x readouts retain their six-digit position resolution"
    );
    for endpoint in [0.0, 1.0] {
        assert!(
            (x.at(x.index(endpoint)) - endpoint).abs() < 1e-12,
            "padding retains the source lattice's integer positions"
        );
    }
    for grid in [&x, &y] {
        assert!(grid.steps >= 4095);
        assert_eq!(grid.resolution, None);
    }
    let panel = &record["panels"][0];
    let shape = &panel["groups"][0]["Drawn"];
    assert!(shape.get("h").is_none() && shape.get("scale").is_none() && shape.get("hs").is_none());
    for p in shape["grid"].as_array().unwrap() {
        for (coordinate, grid) in p.as_array().unwrap().iter().zip([&x, &y]) {
            assert!(coordinate["grid"].as_u64().unwrap() <= u64::from(grid.steps));
        }
    }
    let readouts = panel["readouts"].as_array().unwrap();
    assert!(readouts.len() <= 257, "bounded exact-source anchors");
    assert_eq!(readouts.iter().filter(|r| r["full"] == true).count(), 17);
    let mid = readouts
        .iter()
        .find(|r| r["full"] == true && r["heading"] == "0.5")
        .unwrap();
    assert_eq!(mid["values"][0][1], json!("0.483941"));
    assert_eq!(line(observations(&g)[8], "Density"), Some("0.483941"));
    for r in readouts {
        assert!(r["x"]["grid"].as_u64().unwrap() <= u64::from(x.steps));
        for v in r["values"].as_array().unwrap().iter().filter(|v| !v.is_null()) {
            assert!(v[0]["grid"].as_u64().unwrap() <= u64::from(y.steps));
        }
    }
    assert_eq!(
        g.derived,
        common::draw_at(&spec, &rows, 1440.0).derived,
        "source records are independent of axis tick/layout choices"
    );
    common::runtime_matches_native(&spec, &rows);
}

#[test]
fn density_readout_overflow_fails_even_when_the_drawn_curve_samples_are_finite() {
    // The 128 curve samples miss x=0, where this two-point estimate exceeds f64::MAX.
    let spec = json!({"type":"density","title":"T","encoding":{"x":"v"},"style":{"bandwidth":1.943971540236926e-309}});
    let rows = vec![json!({"v":-1e-309}), json!({"v":1e-309})];
    let g = draw(&spec, &rows);
    assert!(
        g.failure.as_deref().is_some_and(|s| s.contains("extreme")),
        "an unrepresentable exact readout must not become a null/zero density"
    );
    assert!(common::nonfinite(&g).is_none());
}

#[test]
fn data_extents_and_vector_spacing_positions_declare_source_grids() {
    use richview_chart::payload::Quant;
    let fixtures = [
        (
            json!({"type":"scatter3d","title":"T","encoding":{"x":"x","y":"y","z":"z"}}),
            vec![
                json!({"x":1000001.1,"y":1.23456789,"z":3.4567891}),
                json!({"x":1000001.9,"y":9.87654321,"z":4.5678912}),
            ],
            "scatter3d.extents",
        ),
        (
            json!({"type":"vector","title":"T","encoding":{"x":"x","y":"y","angle":"a"}}),
            vec![
                json!({"x":1000001.1,"y":1.23456789,"a":45}),
                json!({"x":1000001.9,"y":9.87654321,"a":90}),
            ],
            "vector.field",
        ),
        (
            json!({"type":"gantt","title":"T","encoding":{"label":"t","start":"s","end":"e"}}),
            vec![json!({"t":"A","s":1000001.1,"e":1000001.9})],
            "gantt.extent",
        ),
    ];
    for (spec, rows, key) in fixtures {
        let g = draw(&spec, &rows);
        let record = &g.derived.iter().find(|d| d.key == key).unwrap().value;
        let ranges: Vec<&Value> = match key {
            "scatter3d.extents" => record.as_array().unwrap().iter().collect(),
            "vector.field" => vec![&record["x"], &record["y"]],
            _ => vec![record],
        };
        for range in &ranges {
            let grid: Quant =
                serde_json::from_value(range["grid"].clone()).expect("a data extent declares its source grid");
            let indices = range["extent"].as_array().unwrap();
            let bounds: Vec<_> = indices
                .iter()
                .map(|v| {
                    let i = v["grid"].as_u64().unwrap();
                    assert!(i <= u64::from(grid.steps));
                    grid.at(i)
                })
                .collect();
            assert!(
                bounds[1] > bounds[0],
                "a narrow nonconstant range survives at its full magnitude"
            );
        }
        if key == "vector.field" {
            for point in record["sample"].as_array().unwrap() {
                for (coordinate, range) in point.as_array().unwrap().iter().zip(&ranges) {
                    let grid: Quant = serde_json::from_value(range["grid"].clone()).unwrap();
                    assert!(coordinate["grid"].as_u64().unwrap() <= u64::from(grid.steps));
                }
            }
        }
        common::runtime_matches_native(&spec, &rows);
    }
}

/// The numbers in `record` that round a source value at more than four significant digits.
fn disclosed(record: &Value, sources: &[f64]) -> Vec<f64> {
    let mut found = Vec::new();
    let mut walk = vec![record];
    while let Some(v) = walk.pop() {
        match v {
            Value::Number(n) => {
                let Some(x) = n.as_f64().filter(|x| *x != 0.0) else {
                    continue;
                };
                let text = format!("{:e}", x.abs());
                let digits = text
                    .split('e')
                    .next()
                    .unwrap()
                    .chars()
                    .filter(char::is_ascii_digit)
                    .count();
                if digits > 4
                    && sources.iter().any(|s| {
                        let r = numbers::round_sig(*s, digits as u8);
                        (r - x).abs() <= 1e-12 * r.abs().max(x.abs())
                    })
                {
                    found.push(x);
                }
            }
            Value::Array(list) => walk.extend(list),
            Value::Object(map) => walk.extend(map.values()),
            _ => {}
        }
    }
    found
}

#[test]
fn contour_and_surface_records_carry_grid_positions_not_source_digits() {
    // A complete grid and the same rows less one, which are interpolated; every value is written
    // to nine significant digits and shown at three (FEA-CH-37).
    let complete: Vec<Value> = (0..4)
        .flat_map(|i| {
            (0..3).map(move |j| {
                json!({"x": 1.00012346 + f64::from(i), "y": 2.00024691 + f64::from(j),
                    "z": 12.3456789 + f64::from(i * 3 + j) * 1.11111111})
            })
        })
        .collect();
    let scattered = &complete[..complete.len() - 1];
    let sources: Vec<f64> = complete
        .iter()
        .flat_map(|r| ["x", "y", "z"].map(|f| r[f].as_f64().unwrap()))
        .collect();
    for (kind, key) in [("contour", "contour.surface"), ("surface", "surface.mesh")] {
        let spec = json!({"type":kind,"title":"T","encoding":{"x":"x","y":"y","z":"z"}});
        for rows in [&complete[..], scattered] {
            let g = draw(&spec, rows);
            assert_eq!(g.failure, None, "{kind}");
            let record = &g.derived.iter().find(|d| d.key == key).expect("the record").value;
            assert_eq!(
                disclosed(record, &sources),
                Vec::<f64>::new(),
                "{kind}, {} rows: {record}",
                rows.len()
            );
            common::runtime_matches_native(&spec, rows);
        }
    }
}

#[test]
fn ecdf_levels_and_strip_cells_carry_grid_positions_not_source_digits() {
    // Values written to nine significant digits and shown at three (FEA-CH-37), with a tie.
    let values = [
        61.7076173, 69.1085309, 71.1087778, 72.608963, 73.0090124, 73.0090124, 78.0096296,
    ];
    let rows: Vec<Value> = values
        .iter()
        .enumerate()
        .map(|(i, v)| json!({"group": if i % 2 == 0 { "A" } else { "B" }, "ms": v}))
        .collect();
    for (spec, key) in [
        (
            json!({"type":"ecdf","title":"T","encoding":{"x":"ms"}}),
            "ecdf.levels.0",
        ),
        (
            json!({"type":"strip","title":"T","encoding":{"x":"group","y":"ms"}}),
            "strip.cells",
        ),
    ] {
        let g = draw(&spec, &rows);
        assert_eq!(g.failure, None, "{key}");
        let record = &g.derived.iter().find(|d| d.key == key).expect("the record").value;
        assert_eq!(disclosed(record, &values), Vec::<f64>::new(), "{key}: {record}");
        common::runtime_matches_native(&spec, &rows);
    }
}

/// A record's grid and the grid steps of its positions (`{"grid": k}`).
fn grid_positions(record: &Value) -> (richview_chart::payload::Quant, Vec<u64>) {
    let (mut grid, mut steps) = (None, Vec::new());
    let mut walk = vec![record];
    while let Some(v) = walk.pop() {
        match v {
            Value::Object(map) => match map.get("grid") {
                Some(Value::Number(k)) if map.len() == 1 => steps.push(k.as_u64().expect("a grid step")),
                Some(quant @ Value::Object(_)) if grid.is_none() => {
                    grid = serde_json::from_value(quant.clone()).ok();
                    walk.extend(map.iter().filter(|(k, _)| *k != "grid").map(|(_, v)| v));
                }
                _ => walk.extend(map.values()),
            },
            Value::Array(list) => walk.extend(list),
            _ => {}
        }
    }
    (grid.expect("the record's grid"), steps)
}

#[test]
fn a_refined_grid_places_record_values_no_finer_than_they_show() {
    // Whole numbers under 10,000 show their units (FEA-CH-22), so the value grid refines to steps
    // of one; from 10,000 a value shows three digits, and its position says no more than its shown
    // value or the base grid's step does (TYP-36, FEA-CH-37).
    use richview_chart::payload::{QUANT_STEPS, Quant};
    let values = [650.08, 9501.17, 10_001.234_6, 10_045.67, 33_333.3, 65_501.2];
    let rows: Vec<Value> = values
        .iter()
        .enumerate()
        .map(|(i, v)| json!({"group": if i % 2 == 0 { "A" } else { "B" }, "ms": v}))
        .collect();
    for (spec, key) in [
        (
            json!({"type":"ecdf","title":"T","encoding":{"x":"ms"}}),
            "ecdf.levels.0",
        ),
        (
            json!({"type":"strip","title":"T","encoding":{"x":"group","y":"ms"}}),
            "strip.cells",
        ),
    ] {
        let g = draw(&spec, &rows);
        assert_eq!(g.failure, None, "{key}");
        let record = &g.derived.iter().find(|d| d.key == key).expect("the record").value;
        let (grid, steps) = grid_positions(record);
        assert_eq!((grid.min, grid.max, grid.steps), (0.0, 66_000.0, 66_000), "{key}");
        assert_eq!(steps.len(), values.len(), "{key}: {record}");
        let base = Quant {
            steps: QUANT_STEPS,
            ..grid.clone()
        };
        for k in steps {
            let p = grid.at(k);
            assert!(
                p == numbers::shown(p, &Format::default()) || (p - base.snap(p)).abs() <= 0.5,
                "{key}: {p} says more than its shown value or the base grid"
            );
        }
        common::runtime_matches_native(&spec, &rows);
    }
}
