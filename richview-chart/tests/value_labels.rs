//! Automatic values print the defined readout measure without sacrificing marks or series identity.
mod common;

use common::*;
use richview_chart::geometry::{Face, Geometry, Item, Role, Text};
use serde_json::json;

fn values(g: &Geometry) -> Vec<&Text> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label && t.face == Face::Figures && t.series.is_some() => Some(t),
            _ => None,
        })
        .collect()
}

#[test]
fn bars_print_the_exact_primary_readout_inside_their_own_ink() {
    let spec = json!({"type":"bar","title":"Amounts","encoding":{"x":"x","y":"v"}});
    let rows = [json!({"x":"A","v":28.1}), json!({"x":"B","v":68.2})];
    for width in [390.0, 1440.0] {
        let g = draw_at(&spec, &rows, width);
        let labels = values(&g);
        assert_eq!(labels.len(), 2);
        for t in labels {
            assert!(
                observations(&g)
                    .iter()
                    .any(|o| o.readout.lines.iter().any(|l| l.value == t.text))
            );
            assert!(
                marks(&g)
                    .iter()
                    .any(|m| matches!(m, Item::Rect{rect,..} if t.bounds.inflate(2.0).inside(rect,0.0)))
            );
            assert!(t.mark.as_ref().is_some_and(|m| g.mark_ids().contains(m)));
        }
    }
    contract(&spec, &rows);
}

#[test]
fn narrow_two_series_label_both_last_values_and_no_first_value() {
    for kind in ["line", "area"] {
        let mut spec = json!({"type":kind,"title":"Output","encoding":{"x":"x","y":"v","color":"s"}});
        if kind == "area" {
            // Numeric area values must stay outside both fills, with room beyond their ends.
            spec["style"] = json!({"ratio":1});
            spec["axes"] = json!({"y":{"min":-100,"max":100}});
        }
        // Opposite-sign area bands leave adjacent white space for both endpoint values.
        let sign = if kind == "area" { -1 } else { 1 };
        let rows = [
            json!({"x":0,"v":12,"s":"A"}),
            json!({"x":1,"v":18,"s":"A"}),
            json!({"x":0,"v":42*sign,"s":"B"}),
            json!({"x":1,"v":36*sign,"s":"B"}),
        ];
        let g = draw_at(&spec, &rows, 390.0);
        let mut texts: Vec<_> = values(&g).iter().map(|t| t.text.as_str()).collect();
        texts.sort();
        let expected = if kind == "area" { ["−36", "18"] } else { ["18", "36"] };
        let mut expected = expected;
        expected.sort();
        assert_eq!(
            texts, expected,
            "{kind}: complete last-value group, not three arbitrary endpoints; plot {:?}, drops {:?}",
            g.panels[0].plot, g.dropped
        );
        contract(&spec, &rows);
    }
}

#[test]
fn overlapping_areas_drop_the_whole_group_instead_of_covering_another_band() {
    let spec = json!({"type":"area","title":"Output","encoding":{"x":"x","y":"v","color":"s"}});
    let rows = [
        json!({"x":0,"v":12,"s":"A"}),
        json!({"x":1,"v":18,"s":"A"}),
        json!({"x":0,"v":42,"s":"B"}),
        json!({"x":1,"v":36,"s":"B"}),
    ];
    let g = draw_at(&spec, &rows, 390.0);
    assert!(values(&g).is_empty(), "a lower curve is inside another filled band");
    assert!(
        g.dropped
            .iter()
            .any(|d| d.kind.as_deref() == Some("value") && d.reason == richview_chart::geometry::DropReason::Collision)
    );
}

#[test]
fn a_budget_too_small_for_all_series_emits_no_partial_endpoint_group() {
    let spec = json!({"type":"line","title":"Output","encoding":{"x":"x","y":"v","color":"s"}});
    let rows: Vec<_> = (0..4)
        .flat_map(|s| (0..3).map(move |x| json!({"x":x,"v":10+s*20+x,"s":format!("S{s}")})))
        .collect();
    let g = draw_at(&spec, &rows, 390.0);
    assert!(values(&g).is_empty());
    assert!(
        g.dropped.iter().any(|d| d.kind.as_deref() == Some("value")),
        "density omission is reported"
    );
}

#[test]
fn tiny_stacked_parts_and_stacked_area_numbers_stay_off() {
    let rows = [
        json!({"x":"Q1","v":68,"s":"Core"}),
        json!({"x":"Q1","v":31,"s":"Support"}),
        json!({"x":"Q1","v":1,"s":"Tiny"}),
    ];
    let spec = json!({"type":"bar","stack":true,"title":"Parts","encoding":{"x":"x","y":"v","color":"s"}});
    let g = draw_at(&spec, &rows, 390.0);
    let mut texts: Vec<_> = values(&g).iter().map(|t| t.text.as_str()).collect();
    texts.sort();
    assert_eq!(texts, ["31", "68"]);
    let spec = json!({"type":"area","stack":true,"title":"Parts","encoding":{"x":"x","y":"v","color":"s"}});
    assert!(values(&draw_at(&spec, &rows, 390.0)).is_empty());
}

#[test]
fn density_prints_one_recorded_evaluation_without_changing_its_meaning() {
    let spec = json!({"type":"density","title":"Response","encoding":{"x":"v"}});
    let rows: Vec<_> = [1.0, 2.0, 2.5, 3.0, 3.5, 5.0, 9.0]
        .into_iter()
        .map(|v| json!({"v":v}))
        .collect();
    let g = draw_at(&spec, &rows, 640.0);
    let labels = values(&g);
    assert_eq!(labels.len(), 1);
    assert!(
        observations(&g)
            .iter()
            .any(|o| o.readout.lines.iter().any(|l| l.derived && l.value == labels[0].text))
    );
    contract(&spec, &rows);
}

#[test]
fn a_visible_curve_without_an_exact_point_in_the_zoom_blocks_partial_value_groups() {
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{ChartTheme, Options, layout};
    let spec = json!({"type":"line","title":"Output","style":{"ratio":1,"gaps":"bridge"},"axes":{"y":{"min":0,"max":100}},"encoding":{"x":"x","y":"v","color":"s"}});
    let mut rows: Vec<_> = (0..5).map(|x| json!({"x":x,"v":40+x,"s":"A"})).collect();
    rows.extend([
        json!({"x":0,"v":70,"s":"B"}),
        json!({"x":1,"v":null,"s":"B"}),
        json!({"x":2,"v":null,"s":"B"}),
        json!({"x":3,"v":null,"s":"B"}),
        json!({"x":4,"v":72,"s":"B"}),
    ]);
    let g = layout(
        &ChartBlock::from_value(&spec).unwrap(),
        &rows,
        &ChartTheme::default(),
        &Options {
            width: 640.0,
            zoom: Some([1.0, 3.0]),
            ..Options::default()
        },
    );
    let p = &g.panels[0];
    assert!(
        p.items.iter().any(|i| i.role() == Role::Mark
            && i.series() == Some(1)
            && i.bounds().is_some_and(|b| b.overlaps(&p.plot, 0.0))),
        "B crosses the visible window"
    );
    assert!(
        p.observations.iter().all(|o| o.dots.iter().all(|d| d.series != 1)),
        "no exact B value exists inside this window"
    );
    assert!(
        values(&g).is_empty(),
        "do not label only A or invent a B value on an interpolated curve"
    );
}

#[test]
fn bar_values_stay_in_the_visible_part_when_authored_bounds_clip_the_bar() {
    let spec = json!({"type":"bar","title":"Clipped baseline","style":{"orientation":"vertical"},"axes":{"y":{"min":90,"max":110}},"encoding":{"x":"x","y":"v"}});
    let g = draw_at(&spec, &[json!({"x":"A","v":100})], 390.0);
    let labels = values(&g);
    assert_eq!(
        labels.len(),
        1,
        "the visible upper half has room for its exact primary value"
    );
    assert!(
        labels[0].bounds.inside(&g.panels[0].plot, 0.0),
        "clipped-away bar length is never room for text: {:?}",
        labels[0].bounds
    );
}

#[test]
fn hatched_values_use_the_existing_single_surface_backing() {
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{ChartTheme, Options, layout};
    let spec = json!({"type":"bar","title":"Hatch","encoding":{"x":"x","y":"v"}});
    let theme = ChartTheme {
        decals: true,
        ..ChartTheme::default()
    };
    let g = layout(
        &ChartBlock::from_value(&spec).unwrap(),
        &[json!({"x":"A","v":68})],
        &theme,
        &Options {
            width: 640.0,
            ..Options::default()
        },
    );
    assert!(
        marks(&g)
            .iter()
            .any(|m| matches!(m,Item::Rect{fill,..} if fill.hatch.is_some())),
        "fixture really uses hatches"
    );
    let labels = values(&g);
    assert_eq!(labels.len(), 1);
    let backing = labels[0].bounds.inflate(1.5);
    let count = g.panels[0]
        .items
        .iter()
        .filter(|i| matches!(i,Item::Rect{rect,fill,..} if *rect==backing && (fill.color.alpha()-0.88).abs()<1e-6))
        .count();
    assert_eq!(count, 1, "reuse the common finish pass; do not paint a second backing");
}

#[test]
fn values_keep_clear_of_the_missing_data_rug() {
    let spec = json!({"type":"line","title":"A small gap","encoding":{"x":"x","y":"v"}});
    let rows: Vec<_> = (0..256)
        .map(|x| json!({"x":x,"v":if x==253 {serde_json::Value::Null} else {json!(if x>=254 {0}else{40})}}))
        .collect();
    let g = draw_at(&spec, &rows, 390.0);
    let rugs: Vec<_> = g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Rect {
                rect, role: Role::Rug, ..
            } => Some(rect),
            _ => None,
        })
        .collect();
    assert!(!rugs.is_empty(), "fixture draws a narrow missing-data rug");
    let labels = values(&g);
    assert!(!labels.is_empty(), "a clear endpoint placement exists");
    for text in labels {
        assert!(
            rugs.iter().all(|r| !text.bounds.inflate(2.0).overlaps(r, 0.0)),
            "{} crowds missing-data ink",
            text.text
        );
    }
}

#[test]
fn narrow_line_values_stay_nearer_their_own_endpoint_than_the_other_series() {
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{ChartTheme, Options, layout};
    let block = ChartBlock::from_value(
        &json!({"type":"line","title":"Output","encoding":{"x":"month","y":"mentions","color":"company"}}),
    )
    .unwrap();
    let rows: Vec<_> = ["2026-04", "2026-05", "2026-06"]
        .iter()
        .enumerate()
        .flat_map(|(i, month)| {
            [
                json!({"month":month,"mentions":([1120,1380,1580][i]),"company":"Anthropic"}),
                json!({"month":month,"mentions":([2200,2160,2060][i]),"company":"OpenAI"}),
            ]
        })
        .collect();
    let theme = ChartTheme {
        data_font: "Space Mono, monospace".into(),
        ui_font: "Space Mono, monospace".into(),
        ..ChartTheme::default()
    };
    let g = layout(
        &block,
        &rows,
        &theme,
        &Options {
            width: 314.0,
            ..Options::default()
        },
    );
    let p = &g.panels[0];
    let labels = values(&g);
    assert_eq!(labels.len(), 2, "both last values fit");
    for text in labels {
        let distance = |at: [f64; 2]| {
            (at[0] - at[0].clamp(text.bounds.x, text.bounds.right()))
                .hypot(at[1] - at[1].clamp(text.bounds.y, text.bounds.bottom()))
        };
        let dots = &p.observations.last().unwrap().dots;
        let own = dots.iter().find(|d| Some(d.series) == text.series).unwrap();
        assert!(
            dots.iter()
                .filter(|d| Some(d.series) != text.series)
                .all(|other| distance(own.at) <= distance(other.at)),
            "{} sits nearer another series",
            text.text
        );
    }
}

#[test]
fn the_author_switch_turns_automatic_values_and_default_names_off_and_keeps_highlights() {
    // Bars, lines and density print values by default; `auto_annotations: false` prints none
    // and records no drop, because nothing was left out against the author's wish.
    let bar_rows = [json!({"x":"A","v":28.1}), json!({"x":"B","v":68.2})];
    let line_rows = [json!({"x":0,"v":12}), json!({"x":1,"v":18}), json!({"x":2,"v":15})];
    for (kind, rows) in [("bar", &bar_rows[..]), ("line", &line_rows[..])] {
        let mut spec = json!({"type":kind,"title":"Amounts","encoding":{"x":"x","y":"v"}});
        assert!(
            !values(&draw_at(&spec, rows, 1100.0)).is_empty(),
            "{kind}: on by default"
        );
        spec["auto_annotations"] = json!(true);
        assert!(
            !values(&draw_at(&spec, rows, 1100.0)).is_empty(),
            "{kind}: explicit true"
        );
        spec["auto_annotations"] = json!(false);
        let g = draw_at(&spec, rows, 1100.0);
        assert!(values(&g).is_empty(), "{kind}: off");
        assert!(g.dropped.iter().all(|d| d.kind.as_deref() != Some("value")));
        contract(&spec, rows);
    }

    let names = |g: &Geometry| -> Vec<String> {
        g.panels
            .iter()
            .flat_map(|p| &p.items)
            .filter_map(|i| match i {
                Item::Text(t) if t.role == Role::Label && t.face == Face::Data => Some(t.text.clone()),
                _ => None,
            })
            .collect()
    };
    let rows: Vec<_> = [("Ada", 1, 9), ("Bo", 4, 2), ("Cy", 7, 6), ("Di", 9, 1)]
        .iter()
        .map(|(n, x, y)| json!({"n":n,"x":x,"y":y}))
        .collect();
    let mut spec = json!({"type":"scatter","title":"Points","encoding":{"x":"x","y":"y","label":"n"}});
    assert_eq!(names(&draw_at(&spec, &rows, 1100.0)).len(), 4, "default names");
    spec["auto_annotations"] = json!(false);
    let g = draw_at(&spec, &rows, 1100.0);
    assert!(names(&g).is_empty(), "no default names: {:?}", names(&g));
    assert!(g.dropped.is_empty(), "an author choice is not a drop: {:?}", g.dropped);
    spec["highlight"] = json!({"by":"n","include":["Cy"]});
    assert_eq!(names(&draw_at(&spec, &rows, 1100.0)), ["Cy"], "highlights stay");
    contract(&spec, &rows);
}
