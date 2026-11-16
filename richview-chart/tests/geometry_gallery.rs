//! Renders the fixture charts through the geometry model and its vector renderer. Set
//! `RICHVIEW_GALLERY_OUT` to a directory to write the SVGs for visual review.

use richview_chart::block::ChartBlock;
use richview_chart::layout::{ChartTheme, Options, layout};
use richview_chart::vector;
use serde_json::{Value, json};

fn fixtures() -> Vec<(&'static str, Value, Vec<Value>)> {
    let months = |n: usize| -> Vec<String> { (0..n).map(|i| format!("{}-{:02}", 2023 + i / 12, i % 12 + 1)).collect() };
    let co2: Vec<Value> = months(30)
        .iter()
        .enumerate()
        .map(|(i, m)| json!({"month": m, "ppm": 419.0 + (i as f64 * 0.21) + ((i as f64) * 0.52).sin() * 3.1}))
        .collect();
    let regions: Vec<Value> = months(18)
        .iter()
        .enumerate()
        .flat_map(|(i, m)| {
            let t = i as f64;
            [
                json!({"month": m, "tickets": 1200.0 - t * 31.0 + (t * 0.9).sin() * 40.0, "region": "North"}),
                json!({"month": m, "tickets": 900.0 - t * 12.0 + (t * 0.7).cos() * 30.0, "region": "South"}),
                json!({"month": m, "tickets": 640.0 + t * 8.0, "region": "West"}),
            ]
        })
        .collect();
    let regions_markers = regions.clone();
    let bars: Vec<Value> = [
        ("Search", 41.2),
        ("Social", 27.5),
        ("Email", 18.1),
        ("Direct", 9.4),
        ("Referral", 3.8),
    ]
    .iter()
    .map(|(c, v)| json!({"channel": c, "share": v}))
    .collect();
    let grouped: Vec<Value> = ["2023", "2024", "2025"].iter().flat_map(|y| [("Q1", 1.0), ("Q2", 1.4), ("Q3", 1.2)].iter().map(move |(q, f)| json!({"year": y, "quarter": q, "revenue": 1_000_000.0 * f * (1.0 + y.parse::<f64>().unwrap_or(2023.0) % 2023.0 * 0.2)}))).collect();
    let scatter: Vec<Value> = (0..40).map(|i| { let x = i as f64; json!({"spend": 10.0 + x * 7.3 + (x * 1.7).sin() * 20.0, "revenue": 50.0 + x * 12.1 + (x * 2.3).cos() * 60.0, "team": if i % 2 == 0 { "Alpha" } else { "Beta" }, "name": format!("P{i}")}) }).collect();
    let oceans: Vec<Value> = [
        ("Pacific", 165.25),
        ("Atlantic", 106.46),
        ("Indian", 70.56),
        ("Southern", 21.96),
        ("Arctic", 15.56),
    ]
    .iter()
    .map(|(o, a)| json!({"ocean": o, "area": a}))
    .collect();
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let tips: Vec<Value> = days.iter().enumerate().flat_map(|(d, day)| (0..24).map(move |h| { let x = h as f64; json!({"hour": h.to_string(), "day": day, "tip": 0.12 + 0.05 * ((x - 6.0) * 0.26).sin() + d as f64 * 0.004})})).collect();
    let small_heat: Vec<Value> = ["North", "South", "West"].iter().enumerate().flat_map(|(r, region)| ["Q1", "Q2", "Q3", "Q4"].iter().enumerate().map(move |(q, quarter)| json!({"quarter": quarter, "region": region, "change": (r as f64 - 1.0) * 4.0 + q as f64 * 1.5 - 2.0}))).collect();
    let times: Vec<Value> = (0..400)
        .map(|i| {
            let u = (i as f64 * 0.618_034).fract();
            let v = (i as f64 * 0.414_214).fract();
            json!({"minutes": 240.0 + 45.0 * (-2.0 * (u + 1e-6).ln()).sqrt() * (std::f64::consts::TAU * v).cos()})
        })
        .collect();
    let latency: Vec<Value> = ["Europe", "Americas", "Asia Pacific"]
        .iter()
        .enumerate()
        .flat_map(|(r, region)| {
            (0..60).map(move |i| {
                let u = (i as f64 * 0.618_034 + r as f64 * 0.1).fract();
                json!({"region": region, "ms": 80.0 + r as f64 * 25.0 + 60.0 * u * u})
            })
        })
        .collect();
    let inflation: Vec<Value> = [
        ("2024", 2.4, 2.4, 2.4),
        ("2025", 2.5, 2.1, 2.9),
        ("2026", 2.2, 1.4, 3.1),
        ("2027", 2.0, 0.9, 3.2),
    ]
    .iter()
    .map(|(y, e, l, u)| json!({"year": y, "cpi": e, "low": l, "high": u}))
    .collect();
    let latency_long = latency.clone();
    let inflation_h = inflation.clone();
    let cars: Vec<Value> = [
        ("Germany", 142.0),
        ("Japan", 98.0),
        ("United States", 61.0),
        ("South Korea", 54.0),
        ("Mexico", 47.0),
        ("Czech Republic", 29.0),
    ]
    .iter()
    .map(|(c, v)| json!({"country": c, "exports": v}))
    .collect();
    vec![
        (
            "pie",
            json!({"type": "pie", "title": "Ocean area", "encoding": {"x": "ocean", "y": "area"}, "axes": {"y": {"label": "Area", "unit": "million km²"}}}),
            oceans,
        ),
        (
            "heatmap-small-cells",
            json!({"type": "heatmap", "title": "Tips", "encoding": {"x": "hour", "y": "day", "color": "tip"}, "axes": {"color": {"format": "percent", "label": "Tip"}, "x": {"label": "Hour"}}}),
            tips,
        ),
        (
            "heatmap-values",
            json!({"type": "heatmap", "title": "Change", "encoding": {"x": "quarter", "y": "region", "color": "change"}, "axes": {"color": {"unit": "%"}}}),
            small_heat,
        ),
        (
            "histogram",
            json!({"type": "histogram", "title": "Finishing times", "encoding": {"x": "minutes"}, "axes": {"x": {"label": "Minutes"}}}),
            times,
        ),
        (
            "boxplot",
            json!({"type": "boxplot", "title": "Latency", "encoding": {"x": "region", "y": "ms"}, "axes": {"y": {"unit": "ms"}}}),
            latency,
        ),
        (
            "interval",
            json!({"type": "interval", "title": "CPI", "encoding": {"x": "year", "y": "cpi", "lower": "low", "upper": "high"}, "axes": {"y": {"unit": "%"}}}),
            inflation,
        ),
        (
            "bar-horizontal",
            json!({"type": "bar", "title": "Car exports", "encoding": {"x": "country", "y": "exports"}, "style": {"orientation": "horizontal"}, "axes": {"y": {"label": "Exports, € billion"}}}),
            cars,
        ),
        (
            "line-calendar",
            json!({"type": "line", "title": "CO2", "encoding": {"x": "month", "y": "ppm"}, "axes": {"y": {"label": "Parts per million", "unit": "ppm"}}}),
            co2.clone(),
        ),
        (
            "line-series",
            json!({"type": "line", "title": "Tickets", "encoding": {"x": "month", "y": "tickets", "color": "region"}}),
            regions.clone(),
        ),
        (
            "area-stacked",
            json!({"type": "area", "title": "Tickets", "stack": true, "encoding": {"x": "month", "y": "tickets", "color": "region"}}),
            regions,
        ),
        (
            "bar",
            json!({"type": "bar", "title": "Share", "encoding": {"x": "channel", "y": "share"}, "axes": {"y": {"unit": "%"}}}),
            bars,
        ),
        (
            "bar-grouped",
            json!({"type": "bar", "title": "Revenue", "encoding": {"x": "quarter", "y": "revenue", "color": "year"}, "axes": {"y": {"unit": "$"}}}),
            grouped,
        ),
        (
            "scatter",
            json!({"type": "scatter", "title": "Spend", "encoding": {"x": "spend", "y": "revenue", "color": "team", "label": "name"}, "axes": {"x": {"label": "Spend"}, "y": {"label": "Revenue"}}}),
            scatter.clone(),
        ),
        (
            "boxplot-horizontal-long-names",
            json!({"type": "boxplot", "title": "Latency", "style": {"orientation": "horizontal"}, "encoding": {"x": "region", "y": "ms"}, "axes": {"y": {"label": "Duration"}}}),
            latency_long
                .iter()
                .map(|r| {
                    let mut r = r.clone();
                    let name = format!("A longer {} region name", r["region"].as_str().unwrap_or(""));
                    r["region"] = json!(name);
                    r
                })
                .collect(),
        ),
        (
            "interval-horizontal",
            json!({"type": "interval", "title": "CPI", "style": {"orientation": "horizontal"}, "encoding": {"x": "year", "y": "cpi", "lower": "low", "upper": "high"}, "axes": {"y": {"label": "Estimate"}}}),
            inflation_h,
        ),
        (
            "line-decals",
            json!({"type": "line", "title": "Tickets", "encoding": {"x": "month", "y": "tickets", "color": "region"}, "style": {"palette": ["#1b6f9e", "#1d729f", "#1f74a1"]}}),
            regions_markers,
        ),
        (
            "scatter-long-x-title",
            json!({"type": "scatter", "title": "Spend", "encoding": {"x": "spend", "y": "revenue"}, "axes": {"x": {"label": "GDP per capita (international dollars, invented for this example)", "unit": "$"}}}),
            scatter,
        ),
    ]
}

#[test]
fn every_fixture_draws_without_geometry_warnings() {
    let theme = ChartTheme::default();
    let out = std::env::var("RICHVIEW_GALLERY_OUT").ok();
    for (name, block, rows) in fixtures() {
        let block = ChartBlock::from_value(&block).expect("block");
        for width in [320.0, 390.0, 640.0, 1440.0] {
            let g = layout(
                &block,
                &rows,
                &theme,
                &Options {
                    width,
                    ..Options::default()
                },
            );
            assert!(g.failure.is_none(), "{name} at {width}: {:?}", g.failure);
            let problems: Vec<_> = g
                .checks
                .iter()
                .filter(|c| matches!(c.kind.as_str(), "clipping" | "label-collision"))
                .map(|c| c.message.clone())
                .collect();
            assert!(problems.is_empty(), "{name} at {width}: {problems:?}");
            if let Some(dir) = &out {
                std::fs::create_dir_all(dir).expect("dir");
                std::fs::write(format!("{dir}/{name}-{width}.svg"), vector::svg(&g, &theme)).expect("write");
            }
        }
    }
}

#[test]
fn print_palette_checks() {
    let theme = ChartTheme::default();
    for (name, block, rows) in fixtures() {
        let block = ChartBlock::from_value(&block).expect("block");
        let g = layout(
            &block,
            &rows,
            &theme,
            &Options {
                width: 640.0,
                ..Options::default()
            },
        );
        for c in &g.checks {
            println!("{name}: {} {}", c.kind, c.message);
        }
    }
}

/// A reader can point at every observation: its own box, wedge or position finds it, at every
/// width (FEA-CH-40, FEA-CH-41).
#[test]
fn every_observation_is_reachable_by_pointer() {
    use richview_chart::geometry::Targeting;
    use richview_chart::hit;
    let theme = ChartTheme::default();
    for (name, block, rows) in fixtures() {
        let block = ChartBlock::from_value(&block).expect("block");
        for width in [320.0, 390.0, 640.0, 1440.0] {
            let g = layout(
                &block,
                &rows,
                &theme,
                &Options {
                    width,
                    ..Options::default()
                },
            );
            for (pi, panel) in g.panels.iter().enumerate() {
                for (oi, o) in panel.observations.iter().enumerate() {
                    let point = match (&o.wedge, &o.area) {
                        (Some(w), _) => {
                            let mid = (w.start + w.end) / 2.0;
                            let r = w.radius * 0.6;
                            [w.center[0] + r * mid.sin(), w.center[1] - r * mid.cos()]
                        }
                        (None, Some(a)) => [a.x + a.w / 2.0, a.y + a.h / 2.0],
                        (None, None) => o.at,
                    };
                    let found = hit::at(&g, point);
                    let target = found.unwrap_or_else(|| {
                        panic!("{name} at {width}: observation {oi} of panel {pi} at {point:?} is unreachable")
                    });
                    let reached = &g.panels[target.panel].observations[target.observation];
                    if panel.targeting == Targeting::NearestX {
                        assert_eq!(reached.x, o.x, "{name} at {width}: observation {oi}");
                    } else {
                        assert_eq!(
                            reached.readout.heading, o.readout.heading,
                            "{name} at {width}: observation {oi}"
                        );
                    }
                }
            }
        }
    }
}

/// Nothing a chart draws leaves its box, and no two labels sit on each other, at any width
/// (FEA-CH-26, FEA-CH-29, FEA-CH-58).
#[test]
fn every_item_stays_inside_the_chart_and_labels_never_overlap() {
    use richview_chart::geometry::{Item, Role};
    let theme = ChartTheme::default();
    for (name, block, rows) in fixtures() {
        let block = ChartBlock::from_value(&block).expect("block");
        for width in [320.0, 358.0, 390.0, 512.0, 640.0, 1440.0] {
            let g = layout(
                &block,
                &rows,
                &theme,
                &Options {
                    width,
                    ..Options::default()
                },
            );
            let items: Vec<&Item> = g.items().collect();
            // Paths count their on-curve points: arc control points lie outside the arc.
            let painted = |item: &Item| match item {
                Item::Line { path, stroke, .. } => on_curve(path).map(|r| r.inflate(stroke.width / 2.0)),
                Item::Area { path, .. } => on_curve(path),
                _ => item.bounds(),
            };
            for item in &items {
                if let Some(b) = painted(item) {
                    assert!(
                        b.x >= -1.0 && b.y >= -1.0 && b.right() <= g.width + 1.0 && b.bottom() <= g.height + 1.0,
                        "{name} at {width}: {:?} {:?} leaves the {}x{} chart: {b:?}",
                        item.role(),
                        if let Item::Text(t) = item { t.text.as_str() } else { "" },
                        g.width,
                        g.height
                    );
                }
            }
            let labels: Vec<_> = items
                .iter()
                .filter_map(|i| match i {
                    Item::Text(t)
                        if t.rotate == 0.0
                            && matches!(
                                t.role,
                                Role::Label
                                    | Role::AxisLabel
                                    | Role::AxisTitle
                                    | Role::Annotation
                                    | Role::PanelTitle
                                    | Role::Key
                            ) =>
                    {
                        Some(t)
                    }
                    _ => None,
                })
                .collect();
            for (a, x) in labels.iter().enumerate() {
                for y in &labels[a + 1..] {
                    assert!(
                        !x.bounds.overlaps(&y.bounds, 1.0),
                        "{name} at {width}: '{}' and '{}' overlap",
                        x.text,
                        y.text
                    );
                }
            }
        }
    }
}

fn on_curve(path: &richview_chart::geometry::Path) -> Option<richview_chart::geometry::Rect> {
    use richview_chart::geometry::{Cmd, Rect};
    let points: Vec<[f64; 2]> = path
        .cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::M { p } | Cmd::L { p } | Cmd::C { p, .. } => Some(*p),
            Cmd::Z => None,
        })
        .collect();
    let (x0, x1) = points
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
    let (y0, y1) = points
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
    (!points.is_empty()).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}
