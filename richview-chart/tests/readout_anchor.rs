//! Readout anchors follow the visible value end of each bar.

use richview_chart::block::ChartBlock;
use richview_chart::layout::{ChartTheme, Options, layout};
use serde_json::json;

#[test]
fn bar_observation_anchors_follow_the_value_end() {
    let rows = [json!({"x":"Gain","y":10}), json!({"x":"Loss","y":-5})];
    for horizontal in [false, true] {
        let block = ChartBlock::from_value(&json!({"type":"bar","title":"T", "style": {"orientation": if horizontal { "horizontal" } else { "vertical" }}, "encoding":{"x":"x","y":"y"}})).unwrap();
        let geometry = layout(&block, &rows, &ChartTheme::default(), &Options::default());
        assert_eq!(geometry.panels[0].observations.len(), 2);
        for (observation, negative) in geometry.panels[0].observations.iter().zip([false, true]) {
            let area = observation.area.unwrap();
            let expected = if horizontal {
                if negative { area.x } else { area.right() }
            } else if negative {
                area.bottom()
            } else {
                area.y
            };
            let actual = if horizontal {
                observation.at[0]
            } else {
                observation.at[1]
            };
            assert_eq!(actual, expected, "horizontal={horizontal} negative={negative}");
        }
    }
}
