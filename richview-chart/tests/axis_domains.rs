//! Authored bounds and missing data must never invert an axis or panic the renderer.
use richview_chart::axis::{ValueSpec, Zero, value_axis};
use richview_chart::block::ChartBlock;
use richview_chart::layout::{ChartTheme, Options, layout};
use richview_chart::numbers::Format;
use richview_chart::text::Font;
use serde_json::json;

#[test]
fn a_missing_value_field_with_an_authored_minimum_does_not_panic() {
    let block = ChartBlock::from_value(&json!({
        "type":"line", "title":"Life expectancy", "encoding":{"x":"year","y":"lif","color":"region"},
        "axes":{"x":{"format":"year"},"y":{"min":40}}
    }))
    .unwrap();
    let rows = [
        json!({"year":1950,"life":42.8,"region":"Africa"}),
        json!({"year":1960,"life":44.0,"region":"Africa"}),
    ];
    let geometry = layout(&block, &rows, &ChartTheme::default(), &Options::default());
    assert!(geometry.width.is_finite() && geometry.height.is_finite());
}

#[test]
fn axis_domains_remain_finite_and_ordered_for_empty_inverted_and_authored_ranges() {
    let format = Format::default();
    let font = Font::new("Geist Mono");
    for data in [
        [0.0, 1.0],
        [f64::INFINITY, f64::NEG_INFINITY],
        [10.0, -10.0],
        [5.0, 5.0],
    ] {
        for min in [None, Some(-10.0), Some(0.0), Some(40.0), Some(f64::NAN)] {
            for max in [None, Some(-10.0), Some(5.0), Some(40.0), Some(f64::NAN)] {
                for zero in [Zero::None, Zero::Soft, Zero::Include] {
                    let axis = value_axis(&ValueSpec {
                        data,
                        zero,
                        min,
                        max,
                        length: 300.0,
                        horizontal: false,
                        format: &format,
                        font: &font,
                        size: 12.0,
                    });
                    assert!(
                        axis.lo.is_finite() && axis.hi.is_finite() && axis.lo < axis.hi,
                        "{data:?}, {min:?}, {max:?}, {zero:?}: {axis:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn cartesian_layouts_tolerate_missing_values_and_inverted_bounds() {
    for kind in ["line", "area", "scatter", "bar"] {
        for min in [None, Some(-10.0), Some(40.0)] {
            for max in [None, Some(-10.0), Some(5.0)] {
                for rows in [
                    vec![json!({"x":1,"other":2}), json!({"x":2,"other":3})],
                    vec![json!({"other":1,"y":2}), json!({"other":2,"y":3})],
                    vec![json!({"x":1,"y":null}), json!({"x":2,"y":null})],
                    vec![json!({"x":1,"y":0}), json!({"x":2,"y":1})],
                ] {
                    let spec = json!({"type":kind,"title":"Range safety","encoding":{"x":"x","y":"y"},"axes":{"y":{"min":min,"max":max}}});
                    let block = ChartBlock::from_value(&spec).unwrap();
                    let geometry = layout(&block, &rows, &ChartTheme::default(), &Options::default());
                    assert!(
                        geometry.width.is_finite() && geometry.height.is_finite(),
                        "{kind} {min:?} {max:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn randomized_axis_domains_keep_valid_scale_endpoints() {
    let format = Format::default();
    let font = Font::new("Geist Mono");
    let mut seed = 0x6a09_e667_f3bc_c909u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 200_001) as f64 / 10.0 - 10_000.0
    };
    for _ in 0..1000 {
        let data = [next(), next()];
        let min = Some(next());
        let max = Some(next());
        let axis = value_axis(&ValueSpec {
            data,
            zero: Zero::Soft,
            min,
            max,
            length: 300.0,
            horizontal: false,
            format: &format,
            font: &font,
            size: 12.0,
        });
        assert!(
            axis.lo.is_finite() && axis.hi.is_finite() && axis.lo < axis.hi,
            "{data:?} {min:?} {max:?}: {axis:?}"
        );
        assert!(axis.map(axis.lo, 0.0, 300.0).is_finite() && axis.map(axis.hi, 0.0, 300.0).is_finite());
    }
}
