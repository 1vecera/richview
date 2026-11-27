//! Outgoing observation buffers must not carry data or selection geometry into a new layout.
use super::common;

use common::*;
use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Dot, Rect, Wedge};
use richview_chart::layout::{ChartTheme, Options, Prepared, layout_prepared, layout_prepared_reusing};
use serde_json::json;

#[test]
fn outgoing_buffers_keep_only_the_new_layout_across_shrink_growth_and_reset() {
    let count = richview_chart::limits::max("line", "all-points observations") + 1;
    let rows: Vec<_> = (0..count)
        .map(|i| json!({"x":i,"y":i % 97 + 1,"series":format!("g{}",i % 3),"facet":i % 2}))
        .collect();
    for (kind, points, stack, multi, faceted) in [
        ("line", None, false, false, false),
        ("line", Some("all"), false, false, false),
        ("line", None, true, false, false),
        ("area", None, false, false, false),
        ("scatter", None, false, false, false),
        ("line", None, false, true, false),
        ("line", None, false, true, true),
    ] {
        let mut value = json!({"type":kind,"title":"New observations","encoding":{"x":"x","y":"y"},
            "digits":3,"readout_digits":3,"stack":stack,"style":{"curve":"linear"}});
        if let Some(points) = points {
            value["style"]["points"] = json!(points);
        }
        if multi {
            value["encoding"]["color"] = json!("series");
        }
        if faceted {
            value["encoding"]["facet"] = json!("facet");
        }
        let block = ChartBlock::from_value(&value).unwrap();
        let theme = ChartTheme::default();
        let prepared = Prepared::default();
        let mut old = layout_prepared(&block, &rows, &theme, &Options::default(), &prepared);
        for (width, zoom) in [(390.0, Some([10.0, 20.0])), (1440.0, Some([5.0, 100.0])), (640.0, None)] {
            let buffers = old
                .panels
                .iter_mut()
                .map(|p| {
                    if let Some(first) = p.observations.first_mut() {
                        std::rc::Rc::make_mut(&mut first.readout).heading = "Old readout".into();
                    }
                    for o in &mut p.observations {
                        o.row = Some(usize::MAX);
                        o.series = Some(usize::MAX);
                        o.at = [-999.0; 2];
                        o.x = -999.0;
                        o.radius = 999.0;
                        o.area = Some(Rect::new(-999.0, -999.0, 999.0, 999.0));
                        o.wedge = Some(Wedge {
                            center: [-999.0; 2],
                            radius: 999.0,
                            inner: 0.0,
                            start: 0.0,
                            end: 1.0,
                        });
                        o.dots.push(Dot {
                            at: [-999.0; 2],
                            series: usize::MAX,
                        });
                    }
                    std::mem::take(&mut p.observations)
                })
                .collect();
            let opts = Options {
                width,
                zoom,
                ..Options::default()
            };
            let next = layout_prepared_reusing(&block, &rows, &theme, &opts, &prepared, buffers);
            assert_eq!(
                next,
                layout_prepared(&block, &rows, &theme, &opts, &prepared),
                "{kind} {points:?} {stack} {multi} {faceted} {zoom:?}"
            );
            assert_eq!(
                observations(&next).len(),
                zoom.map_or(count, |[a, b]| (b - a) as usize + 1)
            );
            if zoom == Some([10.0, 20.0]) && !stack {
                let first = observations(&next)[0];
                // At x=10, row10 belongs to g1 when colour encodes the three series.
                let source_series = if multi { 1 } else { 0 };
                let y = first
                    .readout
                    .lines
                    .iter()
                    .find(|line| line.series == Some(source_series))
                    .unwrap();
                assert_eq!(y.value, "11", "{kind} {points:?} {multi} {faceted}");
            }
            old = next;
        }
    }
}
