//! Histograms: bins that touch, densities whose areas are the authored probabilities, values on
//! an edge in their exact bin, and the contract every type shares (FEA-CH-75).

use super::common;

use serde_json::{Value, json};

fn histogram(rows: Value) -> Value {
    json!({"type":"histogram","title":"Observed distribution","encoding":{"x":"value"},"data":{"inline":rows}})
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let rows: Vec<Value> = [1.2, 2.5, 2.7, 3.1, 3.3, 3.8, 4.4, 5.0, 5.9, 7.2]
        .iter()
        .map(|v| json!({"value": v}))
        .collect();
    common::contract(&histogram(json!(rows)), &rows);
}

#[test]
fn density_rectangles_touch_and_keep_authored_probability_areas() {
    use richview_chart::geometry::{Item, Role};

    let rows = json!([
        {"value":0,"group":"A"},
        {"value":0.5,"group":"B"},
        {"value":1,"group":"A"},
        {"value":3,"group":"B"}
    ]);
    for stacked in [false, true] {
        let mut chart = histogram(rows.clone());
        chart["binning"] = json!({"edges":[0,1,3],"measure":"density"});
        if stacked {
            chart["encoding"]["color"] = json!("group");
        }
        for width in common::WIDTHS {
            let geometry = common::draw_at(&chart, rows.as_array().unwrap(), width);
            assert_eq!(geometry.panels.len(), 1);
            let panel = &geometry.panels[0];
            let mut bins = [Vec::new(), Vec::new()];
            for item in &panel.items {
                if let Item::Rect {
                    rect,
                    radius,
                    stroke,
                    role: Role::Mark,
                    obs: Some(bin),
                    ..
                } = item
                {
                    assert_eq!(*radius, [0.0; 4], "histogram bars are square");
                    assert!(stroke.is_none(), "histogram bars carry no outline");
                    bins[*bin].push(rect);
                }
            }
            let mut areas = [0.0; 2];
            let mut heights = [0.0; 2];
            for (bin, rects) in bins.iter_mut().enumerate() {
                assert_eq!(rects.len(), if stacked { 2 } else { 1 });
                let expected_x = panel.plot.x + if bin == 0 { 0.0 } else { panel.plot.w / 3.0 };
                let expected_width = panel.plot.w * if bin == 0 { 1.0 / 3.0 } else { 2.0 / 3.0 };
                for rect in rects.iter() {
                    assert!(
                        (rect.x - expected_x).abs() < 1e-9,
                        "the painted bar starts on its authored edge"
                    );
                    assert!(
                        (rect.w - expected_width).abs() < 1e-9,
                        "the painted width is the interval's width"
                    );
                    areas[bin] += rect.w * rect.h;
                    heights[bin] += rect.h;
                }
                rects.sort_by(|a, b| a.y.total_cmp(&b.y));
                assert!(
                    (rects.last().unwrap().bottom() - panel.plot.bottom()).abs() < 1e-9,
                    "the stack reaches zero"
                );
                for pair in rects.windows(2) {
                    assert!((pair[0].bottom() - pair[1].y).abs() < 1e-9, "stacked groups touch");
                }
            }
            assert!((bins[0][0].right() - bins[1][0].x).abs() < 1e-9, "adjacent bins touch");
            assert!((bins[1][0].right() - panel.plot.right()).abs() < 1e-9);
            assert!(
                (heights[1] * 2.0 - heights[0]).abs() < 1e-9,
                "density heights are 0.5 and 0.25"
            );
            for area in areas {
                assert!(
                    (area / areas.iter().sum::<f64>() - 0.5).abs() < 1e-12,
                    "each bin paints half the probability"
                );
            }
        }
    }
}

#[test]
fn values_on_authored_edges_stay_in_their_exact_bins_on_the_grid() {
    let ages = [18.0, 87.0, 30.0, 40.0, 45.0, 45.0, 45.0, 45.0, 45.0, 45.0, 50.0, 60.0]
        .into_iter()
        .chain([65.0, 65.0, 65.0, 65.0, 70.0, 80.0]);
    let rows: Vec<Value> = ages.map(|age| json!({"age": age})).collect();
    let spec = json!({
        "type":"histogram","title":"Ages","encoding":{"x":"age"},
        "binning":{"edges":[18,25,35,45,55,65,88]}
    });
    // The grid can move 45 and 65 off their edges; the runtime still draws the exact rows' bins.
    common::runtime_matches_native(&spec, &rows);
}
