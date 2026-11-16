//! Data-integrity contracts on the geometry model (DEL-16, FEA-CH-70, FEA-CH-38, FEA-CH-18).
//! They carry the engine rules of the legacy `verify-numeric-safety` browser suite that drove the
//! retired SVG `ChartView`: numeric text too small to represent is a source error at its row and
//! field in every numeric channel and is never drawn as zero; stack totals beyond the finite range
//! fail the chart; extremes and subnormal values stay finite; and a missing value stays missing.

use richview_chart::block::ChartBlock;
use richview_chart::diag::Level;
use richview_chart::geometry::{Check, Cmd, Geometry, Item, Observation};
use richview_chart::layout::{ChartTheme, Options, layout};
use serde_json::{Value, json};

fn draw(spec: Value, rows: &[Value]) -> Geometry {
    let block = ChartBlock::from_value(&spec).expect("block");
    layout(
        &block,
        rows,
        &ChartTheme::default(),
        &Options {
            width: 640.0,
            ..Options::default()
        },
    )
}

fn observations(g: &Geometry) -> Vec<&Observation> {
    g.panels.iter().flat_map(|p| &p.observations).collect()
}

fn errors(g: &Geometry) -> Vec<&Check> {
    g.checks.iter().filter(|c| c.level == Level::Error).collect()
}

/// Every coordinate of every drawn item is finite.
fn finite_items(g: &Geometry) -> bool {
    let ok = |v: f64| v.is_finite();
    g.panels.iter().flat_map(|p| &p.items).all(|i| match i {
        Item::Rect { rect, .. } => [rect.x, rect.y, rect.w, rect.h].into_iter().all(ok),
        Item::Point { center, radius, .. } => center.iter().copied().chain([*radius]).all(ok),
        Item::Line { path, .. } | Item::Area { path, .. } => path.cmds.iter().all(|c| match c {
            Cmd::M { p } | Cmd::L { p } => p.iter().copied().all(ok),
            Cmd::C { c1, c2, p } => c1.iter().chain(c2).chain(p).copied().all(ok),
            Cmd::Z => true,
        }),
        Item::Text(t) => [t.x, t.y].into_iter().all(ok),
    })
}

/// No readout line anywhere reads as zero.
fn never_zero(g: &Geometry) -> bool {
    !observations(g)
        .iter()
        .any(|o| o.readout.lines.iter().any(|l| l.value == "0"))
}

/// The reason a panel or the whole chart gives for not drawing, if any.
fn failure(g: &Geometry) -> Option<&str> {
    g.failure
        .as_deref()
        .or_else(|| g.panels.iter().find_map(|p| p.failure.as_deref()))
}

/// Every numeric channel: one row's cell is `1e-400`. Each channel treats the row as it treats
/// text that is not a number: coordinates and statistics fail the panel, while a pie value, a
/// heatmap colour or a bubble size leaves the row out.
#[test]
fn numeric_text_too_small_to_represent_is_an_error_at_its_row_and_field_in_every_channel() {
    let base = [
        json!({"x": 1, "c": "A", "g": "A", "r": "R", "v": 2, "s": 4, "lo": 1, "hi": 5}),
        json!({"x": 2, "c": "B", "g": "A", "r": "S", "v": 3, "s": 5, "lo": 2, "hi": 6}),
        json!({"x": 3, "c": "C", "g": "A", "r": "T", "v": 4, "s": 6, "lo": 3, "hi": 7}),
    ];
    let interval = json!({"x": "c", "y": "v", "lower": "lo", "upper": "hi"});
    for (name, kind, encoding, field, fails) in [
        ("line y", "line", json!({"x": "x", "y": "v"}), "v", true),
        ("line x", "line", json!({"x": "x", "y": "v"}), "x", true),
        ("area y", "area", json!({"x": "x", "y": "v"}), "v", true),
        ("area x", "area", json!({"x": "x", "y": "v"}), "x", true),
        ("scatter x", "scatter", json!({"x": "x", "y": "v"}), "x", true),
        ("scatter y", "scatter", json!({"x": "x", "y": "v"}), "v", true),
        (
            "scatter size",
            "scatter",
            json!({"x": "x", "y": "v", "size": "s"}),
            "s",
            false,
        ),
        ("bar y", "bar", json!({"x": "c", "y": "v"}), "v", true),
        ("pie y", "pie", json!({"x": "c", "y": "v"}), "v", false),
        (
            "heatmap color",
            "heatmap",
            json!({"x": "c", "y": "r", "color": "v"}),
            "v",
            false,
        ),
        ("histogram x", "histogram", json!({"x": "v"}), "v", true),
        ("boxplot y", "boxplot", json!({"x": "g", "y": "v"}), "v", true),
        ("interval lower", "interval", interval.clone(), "lo", true),
        ("interval upper", "interval", interval.clone(), "hi", true),
        ("interval estimate", "interval", interval.clone(), "v", true),
    ] {
        let mut rows = base.to_vec();
        rows[1][field] = json!("1e-400");
        let g = draw(json!({"type": kind, "title": "T", "encoding": encoding}), &rows);
        let found: Vec<&Check> = errors(&g)
            .into_iter()
            .filter(|c| c.path.as_deref() == Some(&format!("rows[1].{field}")))
            .collect();
        assert_eq!(found.len(), 1, "{name}: one error at rows[1].{field}: {:#?}", g.checks);
        let found = found[0];
        assert_eq!(found.kind, "source-error", "{name}: {found:?}");
        assert!(found.message.contains("Row 2"), "{name}: {}", found.message);
        assert!(found.message.contains("1e-400"), "{name}: {}", found.message);
        assert!(
            found.message.contains("too small to represent"),
            "{name}: {}",
            found.message
        );
        assert!(never_zero(&g), "{name}: the value is never drawn as zero");
        if fails {
            let reason = failure(&g).unwrap_or_else(|| panic!("{name}: the panel draws"));
            assert!(reason.starts_with("Cannot draw"), "{name}: {reason}");
            assert!(reason.contains("too small to represent"), "{name}: {reason}");
        } else {
            assert_eq!(failure(&g), None, "{name}: the chart draws without the row");
            assert!(!observations(&g).is_empty(), "{name}");
        }
    }
}

/// A column whose only values are too small to represent stays numeric and is reported; it never
/// becomes category names (FEA-CH-70).
#[test]
fn a_column_of_only_unrepresentable_values_is_reported_not_read_as_categories() {
    let rows = [json!({"x": "1e-400", "v": 1}), json!({"x": "2e-400", "v": 2})];
    for kind in ["line", "area"] {
        let g = draw(
            json!({"type": kind, "title": "T", "encoding": {"x": "x", "y": "v"}}),
            &rows,
        );
        let paths: Vec<_> = errors(&g).iter().filter_map(|c| c.path.clone()).collect();
        assert_eq!(paths, ["rows[0].x", "rows[1].x"], "{kind}: {:#?}", g.checks);
        assert!(
            failure(&g).is_some_and(|r| r.contains("too small to represent")),
            "{kind}"
        );
    }
}

/// A row with both coordinates too small to represent names both fields in one pass.
#[test]
fn both_coordinates_of_one_row_are_reported() {
    let rows = [
        json!({"x": 1, "v": 1}),
        json!({"x": "1e-400", "v": "1e-400"}),
        json!({"x": 3, "v": 3}),
    ];
    for kind in ["line", "scatter"] {
        let g = draw(
            json!({"type": kind, "title": "T", "encoding": {"x": "x", "y": "v"}}),
            &rows,
        );
        let mut paths: Vec<_> = errors(&g).iter().filter_map(|c| c.path.clone()).collect();
        paths.sort();
        assert_eq!(paths, ["rows[1].v", "rows[1].x"], "{kind}: {:#?}", g.checks);
    }
}

/// Category labels keep their text even when it looks like a number too small to represent.
#[test]
fn numeric_looking_category_labels_keep_their_text() {
    let rows = [json!({"c": "1e-400", "v": 2}), json!({"c": "2", "v": 3})];
    for kind in ["bar", "pie"] {
        let g = draw(
            json!({"type": kind, "title": "T", "encoding": {"x": "c", "y": "v"}}),
            &rows,
        );
        assert!(errors(&g).is_empty(), "{kind}: {:#?}", g.checks);
        assert!(
            observations(&g)
                .iter()
                .any(|o| o.readout.heading == "1e-400" || o.readout.lines.iter().any(|l| l.name == "1e-400")),
            "{kind}: the label reads as written: {:#?}",
            observations(&g).iter().map(|o| &o.readout).collect::<Vec<_>>()
        );
    }
}

/// A failing facet panel fails alone (FEA-CH-18): the other panel still draws.
#[test]
fn an_unrepresentable_value_fails_only_its_facet_panel() {
    let rows = [
        json!({"f": "Left", "x": 1, "v": 2}),
        json!({"f": "Left", "x": 2, "v": "1e-400"}),
        json!({"f": "Right", "x": 1, "v": 4}),
        json!({"f": "Right", "x": 2, "v": 5}),
    ];
    for (kind, encoding) in [
        ("line", json!({"x": "x", "y": "v", "facet": "f"})),
        ("histogram", json!({"x": "v", "facet": "f"})),
    ] {
        let g = draw(json!({"type": kind, "title": "T", "encoding": encoding}), &rows);
        assert_eq!(g.failure, None, "{kind}: the chart as a whole draws");
        assert_eq!(g.panels.len(), 2, "{kind}");
        assert!(
            g.panels[0]
                .failure
                .as_deref()
                .is_some_and(|r| r.contains("too small to represent")),
            "{kind}: {:?}",
            g.panels[0].failure
        );
        assert_eq!(g.panels[1].failure, None, "{kind}");
        assert!(!g.panels[1].observations.is_empty(), "{kind}");
    }
}

/// A grouped calendar series: the error names the source row, not a position after the rows are
/// sorted or grouped, and padded text is read as its number.
#[test]
fn grouped_calendar_underflow_names_the_source_row() {
    let rows = [
        json!({"date": "2024-03", "group": "A", "value": 3}),
        json!({"date": "2024-01", "group": "B", "value": 1}),
        json!({"date": "2024-02", "group": "A", "value": 2}),
        json!({"date": "2024-01", "group": "A", "value": " 1e-400 "}),
    ];
    for kind in ["line", "area"] {
        let g = draw(
            json!({"type": kind, "title": "T", "encoding": {"x": "date", "y": "value", "color": "group"}}),
            &rows,
        );
        let found = errors(&g)
            .into_iter()
            .find(|c| c.path.as_deref() == Some("rows[3].value"))
            .unwrap_or_else(|| panic!("{kind}: no error at rows[3].value: {:#?}", g.checks));
        assert!(found.message.contains("Row 4"), "{kind}: {}", found.message);
        assert!(
            found.message.contains("too small to represent"),
            "{kind}: {}",
            found.message
        );
        assert!(never_zero(&g), "{kind}: never drawn as zero");
    }
}

/// A stack whose total leaves the finite range fails with `value-overflow`, on a calendar axis
/// too (FEA-CH-05, FEA-CH-70).
#[test]
fn a_stacked_calendar_area_whose_total_overflows_fails_with_value_overflow() {
    let rows = [
        json!({"date": "2024-01", "s": "a", "v": 1.5e308}),
        json!({"date": "2024-01", "s": "b", "v": 1.5e308}),
        json!({"date": "2024-02", "s": "a", "v": 1}),
        json!({"date": "2024-02", "s": "b", "v": 2}),
    ];
    for kind in ["area", "bar"] {
        let g = draw(
            json!({"type": kind, "title": "T", "stack": true, "encoding": {"x": "date", "y": "v", "color": "s"}}),
            &rows,
        );
        assert!(
            errors(&g).iter().any(|c| c.kind == "value-overflow"),
            "{kind}: {:#?}",
            g.checks
        );
    }
}

/// Finite extremes and subnormal samples keep every coordinate finite on a calendar axis, and a
/// true zero stays a defined observation (FEA-CH-70).
#[test]
fn calendar_extremes_and_subnormals_stay_finite_and_zero_stays_defined() {
    for values in [[-1e308, 1e308, 0.0], [1e-308, 2e-308, 0.0], [5e-324, 1e308, 0.0]] {
        let rows: Vec<Value> = values
            .iter()
            .enumerate()
            .map(|(i, v)| json!({"date": format!("2024-0{}", i + 1), "v": v}))
            .collect();
        let g = draw(
            json!({"type": "line", "title": "T", "encoding": {"x": "date", "y": "v"}}),
            &rows,
        );
        assert!(errors(&g).is_empty(), "{values:?}: {:#?}", g.checks);
        assert_eq!(observations(&g).len(), 3, "{values:?}");
        for o in observations(&g) {
            assert!(o.at.iter().all(|c| c.is_finite()), "{values:?}: {o:?}");
        }
        assert!(finite_items(&g), "{values:?}: every drawn coordinate is finite");
        let x = g.panels[0].x.as_ref().expect("a calendar scale");
        assert!(x.calendar, "{values:?}");
    }
}

/// A missing value keeps its place and reads "No observation"; it is never drawn at zero
/// (FEA-CH-38).
#[test]
fn a_missing_calendar_value_stays_missing() {
    let rows = [
        json!({"date": "2024-01", "v": 4}),
        json!({"date": "2024-02", "v": null}),
        json!({"date": "2024-03", "v": 6}),
    ];
    let g = draw(
        json!({"type": "line", "title": "T", "encoding": {"x": "date", "y": "v"}}),
        &rows,
    );
    assert!(errors(&g).is_empty(), "{:#?}", g.checks);
    assert!(never_zero(&g));
    let mut obs = observations(&g);
    obs.sort_by(|a, b| a.x.total_cmp(&b.x));
    let values: Vec<&str> = obs.iter().map(|o| o.readout.lines[0].value.as_str()).collect();
    assert_eq!(values, ["4", "No observation", "6"]);
    assert!(obs[1].dots.is_empty(), "no hover dot at the missing month");
}
