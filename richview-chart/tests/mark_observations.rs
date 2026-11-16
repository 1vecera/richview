//! A mark indexes its panel's visible observations, independently of source row order.
use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Item, Role};
use richview_chart::layout::{ChartTheme, Options, layout};
use serde_json::json;

#[test]
fn hidden_series_facets_and_zoom_keep_marks_attached_to_their_visible_observations() {
    let rows = vec![
        json!({"c": "B", "x": 1, "y": 5, "s": "hidden", "f": "one"}),
        json!({"c": "A", "x": 2, "y": 7, "s": "shown", "f": "two"}),
        json!({"c": "B", "x": 3, "y": 9, "s": "shown", "f": "one"}),
        json!({"c": "A", "x": 4, "y": 11, "s": "shown", "f": "one"}),
    ];
    for kind in ["bar", "scatter"] {
        for orientation in ["vertical", "horizontal"] {
            let block = ChartBlock::from_value(&json!({
                "type": kind, "title": "T", "style": {"orientation": orientation},
                "encoding": {"x": if kind == "bar" {"c"} else {"x"}, "y": "y", "color": "s", "facet": "f"}
            }))
            .unwrap();
            let g = layout(
                &block,
                &rows,
                &ChartTheme::default(),
                &Options {
                    width: 640.0,
                    hidden: vec!["hidden".into()],
                    zoom: (kind == "scatter").then_some([2.5, 4.0]),
                    ..Default::default()
                },
            );
            let mut marks = 0;
            for panel in &g.panels {
                for item in &panel.items {
                    let (at, index) = match item {
                        Item::Rect {
                            rect,
                            obs: Some(o),
                            role: Role::Mark,
                            ..
                        } => (rect.center(), *o),
                        Item::Point {
                            center,
                            obs: Some(o),
                            role: Role::Mark,
                            ..
                        } => (*center, *o),
                        _ => continue,
                    };
                    marks += 1;
                    let observation = panel.observations.get(index).unwrap_or_else(|| {
                        panic!(
                            "{kind}/{orientation}: mark {index} has only {} visible observations",
                            panel.observations.len()
                        )
                    });
                    assert!(observation.row.is_some());
                    if let Some(area) = observation.area {
                        assert!(area.contains(at), "{kind}/{orientation}: mark points at another bar");
                    } else {
                        assert_eq!(observation.at, at, "{kind}/{orientation}: mark points at another dot");
                    }
                }
            }
            assert!(marks > 0);
        }
    }
}
