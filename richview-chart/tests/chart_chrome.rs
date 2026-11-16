//! The chart chrome of STY-40, read from the geometry every painter draws: gridlines one pixel in
//! rule-soft, the zero line 1.5 px in the chart ink, ticks one pixel in ink-faint, data lines
//! 2.5 px, areas at 15 % and bands at 28 %. The legend swatches and a hidden series are report
//! CSS (`richview-core` tests/style_rules.rs), and a dimmed series is the runtime's
//! (`richview-runtime` src/mesh.rs).

mod common;

use common::*;
use richview_chart::color;
use richview_chart::geometry::{Fill, Item, Rgba, Role, Stroke};
use richview_chart::layout::ChartTheme;
use serde_json::{Value, json};

fn rgba(css: &str) -> Rgba {
    Rgba::from_oklch(color::parse(css).expect("a theme colour"), 1.0)
}

fn strokes(g: &richview_chart::geometry::Geometry, role: Role) -> Vec<&Stroke> {
    g.items()
        .filter(|i| i.role() == role)
        .filter_map(|i| match i {
            Item::Line { stroke, .. } => Some(stroke),
            _ => None,
        })
        .collect()
}

fn fills(g: &richview_chart::geometry::Geometry, role: Role) -> Vec<&Fill> {
    g.items()
        .filter(|i| i.role() == role)
        .filter_map(|i| match i {
            Item::Area { fill, .. } => Some(fill),
            _ => None,
        })
        .collect()
}

/// A calendar series that crosses zero, so the chart draws gridlines, ticks and a zero line.
fn crossing() -> Vec<Value> {
    [
        ("2026-01", -4),
        ("2026-02", 3),
        ("2026-03", 7),
        ("2026-04", -2),
        ("2026-05", 5),
    ]
    .iter()
    .map(|(m, v)| json!({"month": m, "change": v}))
    .collect()
}

#[test]
fn chart_chrome_is_light_gridlines_hairline_zero_line_firmer() {
    let theme = ChartTheme::default();
    let g = draw(
        &json!({"type": "line", "title": "Change", "encoding": {"x": "month", "y": "change"}}),
        &crossing(),
    );
    let grid = strokes(&g, Role::Grid);
    assert!(!grid.is_empty(), "the chart draws gridlines");
    for s in grid {
        assert_eq!((s.width, s.color), (1.0, rgba(&theme.rule_soft)), "gridlines: {s:?}");
    }
    let zero = strokes(&g, Role::Baseline);
    assert_eq!(zero.len(), 1, "one zero line");
    assert_eq!((zero[0].width, zero[0].color), (1.5, rgba(&theme.ink)));
    let ticks = strokes(&g, Role::Tick);
    assert!(!ticks.is_empty(), "a calendar axis draws ticks");
    for s in ticks {
        assert_eq!((s.width, s.color), (1.0, rgba(&theme.ink_faint)), "ticks: {s:?}");
    }
    let lines = strokes(&g, Role::Mark);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].width, 2.5, "data lines are 2.5 px");
}

#[test]
fn area_fills_are_fifteen_percent_and_bands_twenty_eight() {
    let area = draw(
        &json!({"type": "area", "title": "Change", "encoding": {"x": "month", "y": "change"}}),
        &crossing(),
    );
    let areas = fills(&area, Role::Mark);
    assert!(!areas.is_empty(), "the area chart fills");
    for f in areas {
        assert!((f.color.0[3] - 0.15).abs() < 1e-6, "an area fills at {}", f.color.0[3]);
    }
    let v: Value = serde_json::from_str(include_str!("fixtures/types/band.json")).unwrap();
    let band = draw(&v["spec"], v["rows"].as_array().unwrap());
    let bands: Vec<&Fill> = band
        .items()
        .filter_map(|i| match i {
            Item::Area { fill, .. } => Some(fill),
            _ => None,
        })
        .collect();
    assert!(!bands.is_empty(), "the band chart fills its range");
    for f in bands {
        assert!((f.color.0[3] - 0.28).abs() < 1e-6, "a band fills at {}", f.color.0[3]);
    }
}
