//! `highlight.mute` (TYP-78): the marks `include` matches keep their colour, names and values;
//! the rest take the style's quiet ink, lose the names and values the chart adds by itself, and
//! paint first. On by default for line, area, bar and lollipop charts, off for a scatter.
mod common;

use common::*;
use richview_chart::color;
use richview_chart::geometry::{Face, Geometry, Item, Rgba, Role, Text};
use richview_chart::layout::ChartTheme;
use serde_json::{Value, json};

fn quiet() -> Rgba {
    Rgba::from_oklch(color::parse(&ChartTheme::default().ink_faint).expect("quiet ink"), 1.0)
}

/// The colour a mark is drawn in: a line's stroke, an area's, bar's or point's fill.
fn ink(i: &Item) -> Option<Rgba> {
    match i {
        Item::Line { stroke, .. } => Some(stroke.color),
        Item::Rect { fill, .. } | Item::Area { fill, .. } => Some(fill.color),
        Item::Point { fill, stroke, .. } => fill.as_ref().map(|f| f.color).or(stroke.as_ref().map(|s| s.color)),
        _ => None,
    }
}

/// Texts the chart adds itself: values (figures) and names (data face) that belong to a series.
fn labels(g: &Geometry) -> Vec<&Text> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label && matches!(t.face, Face::Figures | Face::Data) => Some(t),
            _ => None,
        })
        .collect()
}

fn three_lines() -> Vec<Value> {
    let mut rows = Vec::new();
    for (s, base) in [("North", 10.0), ("South", 20.0), ("West", 30.0)] {
        for x in 0..6 {
            rows.push(json!({"x": x, "v": base + (x as f64 * 1.7).sin() * 3.0, "region": s}));
        }
    }
    rows
}

#[test]
fn a_line_chart_mutes_the_series_the_highlight_leaves_out() {
    let rows = three_lines();
    let spec = json!({"type":"line","title":"Sales by region","encoding":{"x":"x","y":"v","color":"region"},
        "highlight":{"by":"region","include":["south"]}});
    for width in [390.0, 1100.0] {
        let g = draw_at(&spec, &rows, width);
        let south = g.series.iter().position(|s| s.key == "South").expect("South");
        for (s, series) in g.series.iter().enumerate() {
            assert_eq!(series.color == quiet(), s != south, "{width}: {} colour", series.key);
        }
        let panel = &g.panels[0];
        let lines: Vec<(usize, usize, Rgba)> = panel
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.role() == Role::Mark && matches!(i, Item::Line { .. }))
            .filter_map(|(k, i)| Some((k, i.series()?, ink(i)?)))
            .collect();
        assert_eq!(lines.len(), 3, "{width}: three lines");
        for &(_, s, c) in &lines {
            assert_eq!(c == quiet(), s != south, "{width}: series {s} ink");
        }
        // The quiet lines paint first: the highlighted line is never under one.
        let last_muted = lines.iter().filter(|l| l.1 != south).map(|l| l.0).max().expect("muted");
        let highlighted = lines.iter().find(|l| l.1 == south).expect("South line").0;
        assert!(last_muted < highlighted, "{width}: South paints last");
        // Only the highlighted series is named or valued, and the legend lists only it.
        let texts = labels(&g);
        assert!(!texts.is_empty(), "{width}: South keeps its name or values");
        for t in texts {
            assert_eq!(t.series, Some(south), "{width}: '{}' belongs to a muted series", t.text);
        }
        if let Some(legend) = &g.legend {
            assert_eq!(legend.entries, [south], "{width}: legend");
        }
        assert!(
            g.dropped.iter().all(|d| d.series.is_none_or(|s| s == south)),
            "{width}: a muted series' values are the author's choice, not drops: {:?}",
            g.dropped
        );
        // The readout still lists every series.
        assert!(
            observations(&g).iter().any(|o| line(o, "North").is_some()),
            "{width}: readout"
        );
    }
    contract(&spec, &rows);
}

#[test]
fn mute_false_keeps_every_colour_and_name() {
    let rows = three_lines();
    let spec = json!({"type":"line","title":"Sales by region","encoding":{"x":"x","y":"v","color":"region"},
        "highlight":{"by":"region","include":["South"],"mute":false}});
    let plain = json!({"type":"line","title":"Sales by region","encoding":{"x":"x","y":"v","color":"region"}});
    let (g, before) = (draw_at(&spec, &rows, 1100.0), draw_at(&plain, &rows, 1100.0));
    assert!(g.series.iter().all(|s| s.color != quiet()));
    assert_eq!(g.series, before.series);
    assert_eq!(g.legend, before.legend);
    assert_eq!(labels(&g).len(), labels(&before).len());
}

#[test]
fn bars_mute_row_by_row_and_keep_the_highlighted_value() {
    let rows: Vec<Value> = [("Alpha", 12.0), ("Beta", 30.0), ("Gamma", 21.0), ("Delta", 8.0)]
        .iter()
        .map(|(x, v)| json!({"team": x, "v": v}))
        .collect();
    let spec = json!({"type":"bar","title":"Tickets closed","encoding":{"x":"team","y":"v"},
        "highlight":{"by":"team","include":["Gamma"]}});
    for width in [390.0, 1100.0] {
        let g = draw_at(&spec, &rows, width);
        let obs = observations(&g);
        let bars: Vec<(Option<usize>, Rgba)> = g.panels[0]
            .items
            .iter()
            .filter(|i| i.role() == Role::Mark && matches!(i, Item::Rect { .. }))
            .filter_map(|i| match i {
                Item::Rect { obs: k, fill, .. } => Some((k.and_then(|k| obs.get(k)?.row), fill.color)),
                _ => None,
            })
            .collect();
        assert_eq!(bars.len(), 4, "{width}: four bars");
        for (row, c) in &bars {
            assert_eq!(*c == quiet(), *row != Some(2), "{width}: row {row:?} ink");
        }
        let values: Vec<&str> = labels(&g)
            .iter()
            .filter(|t| t.face == Face::Figures)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(values, ["21"], "{width}: only Gamma's value");
        assert!(
            g.dropped.iter().all(|d| d.kind.as_deref() != Some("value")),
            "{width}: no drops"
        );
    }
    contract(&spec, &rows);
}

#[test]
fn a_lollipop_mutes_row_by_row() {
    let rows: Vec<Value> = [("Alpha", 12.0), ("Beta", 30.0), ("Gamma", 21.0)]
        .iter()
        .map(|(x, v)| json!({"team": x, "v": v}))
        .collect();
    let spec = json!({"type":"lollipop","title":"Tickets closed","encoding":{"x":"team","y":"v"},
        "highlight":{"by":"team","include":["Beta"]}});
    let g = draw_at(&spec, &rows, 1100.0);
    let dots: Vec<Rgba> = g.panels[0]
        .items
        .iter()
        .filter(|i| i.role() == Role::Mark && matches!(i, Item::Point { .. }))
        .filter_map(ink)
        .collect();
    assert_eq!(dots.iter().filter(|c| **c == quiet()).count(), 2, "two quiet dots");
    let values: Vec<&str> = labels(&g)
        .iter()
        .filter(|t| t.face == Face::Figures)
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(values, ["30"], "only Beta's value");
    contract(&spec, &rows);
}

#[test]
fn a_highlighted_lollipop_near_the_axis_end_keeps_its_value() {
    // Team's −18 sits a few pixels above the axis' −20: its value cannot go below the dot and
    // moves beside it, held inside the plot.
    let rows: Vec<Value> = [
        ("Solo", 42.0),
        ("Team", -18.0),
        ("Business", 65.0),
        ("Scale", -7.0),
        ("Enterprise", 23.0),
    ]
    .iter()
    .map(|(x, v)| json!({"plan": x, "accounts": v}))
    .collect();
    let spec = json!({"type":"lollipop","title":"Net accounts","encoding":{"x":"plan","y":"accounts"},
        "highlight":{"by":"plan","include":["Team","Scale"]}});
    for width in (320..=1300).step_by(4).map(f64::from) {
        let g = draw_at(&spec, &rows, width);
        let plot = g.panels[0].plot;
        let values: Vec<&Text> = labels(&g).into_iter().filter(|t| t.face == Face::Figures).collect();
        let mut texts: Vec<&str> = values.iter().map(|t| t.text.as_str()).collect();
        texts.sort_unstable();
        assert_eq!(texts, ["−18", "−7"], "{width}: both highlighted values");
        for t in &values {
            assert!(
                t.bounds.inside(&plot, 0.5),
                "{width}: '{}' {:?} leaves the plot {plot:?}",
                t.text,
                t.bounds
            );
        }
        assert!(g.dropped.is_empty(), "{width}: drops {:?}", g.dropped);
    }
    contract(&spec, &rows);
}

#[test]
fn a_scatter_keeps_its_colours_unless_the_author_mutes_it() {
    let rows: Vec<Value> = [("Ada", 1, 9), ("Bo", 4, 2), ("Cy", 7, 6), ("Di", 9, 1)]
        .iter()
        .map(|(n, x, y)| json!({"n":n,"x":x,"y":y}))
        .collect();
    let mut spec = json!({"type":"scatter","title":"Points","encoding":{"x":"x","y":"y","label":"n"},
        "highlight":{"by":"n","include":["Cy"]}});
    // A scatter fills its points at the points' opacity: compare the colour, not the alpha.
    let points = |g: &Geometry| -> Vec<Rgba> {
        marks(g)
            .into_iter()
            .filter(|i| matches!(i, Item::Point { .. }))
            .filter_map(ink)
            .map(|c| c.with_alpha(1.0))
            .collect()
    };
    let names = |g: &Geometry| -> Vec<String> {
        labels(g)
            .iter()
            .filter(|t| t.face == Face::Data)
            .map(|t| t.text.clone())
            .collect()
    };
    let g = draw_at(&spec, &rows, 1100.0);
    assert!(points(&g).iter().all(|c| *c != quiet()), "scatter default: no mute");
    assert_eq!(names(&g).len(), 4, "scatter default: highlights and automatic names");
    spec["highlight"]["mute"] = json!(true);
    let g = draw_at(&spec, &rows, 1100.0);
    let quiet_points = points(&g).iter().filter(|c| **c == quiet()).count();
    assert_eq!(quiet_points, 3, "three quiet points");
    assert_eq!(names(&g), ["Cy"], "only the highlight is named");
    // The highlighted point paints last.
    let last = marks(&g)
        .into_iter()
        .rfind(|i| matches!(i, Item::Point { .. }))
        .and_then(ink)
        .map(|c| c.with_alpha(1.0));
    assert!(last.is_some_and(|c| c != quiet()), "Cy on top");
    contract(&spec, &rows);
}

#[test]
fn a_highlight_that_matches_nothing_warns_and_mutes_nothing() {
    let rows: Vec<Value> = [("Alpha", 12.0), ("Beta", 30.0)]
        .iter()
        .map(|(x, v)| json!({"team": x, "v": v}))
        .collect();
    let spec = json!({"type":"bar","title":"Tickets closed","encoding":{"x":"team","y":"v"},
        "highlight":{"by":"team","include":["Omega"]}});
    let g = draw_at(&spec, &rows, 1100.0);
    let warning = check(&g, "unresolved-highlight").expect("a warning");
    assert_eq!(warning.path.as_deref(), Some("chart.highlight.include"));
    assert!(
        marks(&g).into_iter().filter_map(ink).all(|c| c != quiet()),
        "nothing muted"
    );
    assert_eq!(
        labels(&g).iter().filter(|t| t.face == Face::Figures).count(),
        2,
        "both values stay"
    );
}

#[test]
fn with_decals_the_highlighted_line_draws_solid_and_the_muted_ones_keep_their_dashes() {
    let rows = three_lines();
    // South takes slot 1, whose decal is a dash.
    let spec = json!({"type":"line","title":"Sales by region","encoding":{"x":"x","y":"v","color":"region"},
        "highlight":{"by":"region","include":["South"]}});
    let block = richview_chart::block::ChartBlock::from_value(&spec).expect("a chart block");
    let theme: ChartTheme = serde_json::from_value(json!({"decals": true})).expect("theme");
    let g = richview_chart::layout::layout(&block, &rows, &theme, &richview_chart::layout::Options::default());
    let south = g.series.iter().position(|s| s.key == "South").expect("South");
    assert!(g.series[south].slot > 0, "South is not the solid slot by itself");
    assert!(g.series[south].dash.is_empty(), "the highlighted line is solid");
    let dashed = g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Line {
                stroke,
                role: Role::Mark,
                series: Some(s),
                ..
            } => Some((*s, !stroke.dash.is_empty())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        dashed.iter().any(|&(s, d)| s == south && !d),
        "South's stroke is solid: {dashed:?}"
    );
    assert!(
        dashed.iter().any(|&(s, d)| s != south && d),
        "a muted line keeps its dash: {dashed:?}"
    );
}
