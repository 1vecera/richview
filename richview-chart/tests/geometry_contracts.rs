//! Chart-family contracts on the geometry model the WebGL runtime draws as it is (ARC-18,
//! DEL-72). They replace the browser suites that drove the retired SVG chart runtime: bins
//! (FEA-CH-11, TYP-20), box plots and intervals (FEA-CH-14), held and smoothed lines
//! (FEA-CH-02, FEA-CH-04), bar paint (FEA-CH-03), numeric safety (FEA-CH-38) and zoom windows at
//! every floating-point scale (FEA-CH-49).

use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Cmd, Geometry, Item, Observation, Role};
use richview_chart::layout::{ChartTheme, Options, layout};
use serde_json::{Value, json};

fn draw_with(spec: Value, rows: &[Value], theme: &ChartTheme, opts: Options) -> Geometry {
    let block = ChartBlock::from_value(&spec).expect("block");
    let width = if opts.width > 0.0 { opts.width } else { 640.0 };
    layout(&block, rows, theme, &Options { width, ..opts })
}

fn draw(spec: Value, rows: &[Value]) -> Geometry {
    draw_with(spec, rows, &ChartTheme::default(), Options::default())
}

/// A readout value by line name.
fn line<'a>(o: &'a Observation, name: &str) -> Option<&'a str> {
    o.readout
        .lines
        .iter()
        .find(|l| l.name == name)
        .map(|l| l.value.as_str())
}

fn observations(g: &Geometry) -> Vec<&Observation> {
    g.panels.iter().flat_map(|p| &p.observations).collect()
}

fn marks(g: &Geometry) -> Vec<&Item> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| i.role() == Role::Mark)
        .collect()
}

fn check<'a>(g: &'a Geometry, kind: &str) -> Option<&'a richview_chart::geometry::Check> {
    g.checks.iter().find(|c| c.kind == kind)
}

/// Every coordinate of every item is finite.
fn finite(g: &Geometry) -> bool {
    let bad = nonfinite(g);
    if let Some(item) = bad {
        eprintln!("not finite: {item:?}");
    }
    bad.is_none()
}

fn nonfinite(g: &Geometry) -> Option<&Item> {
    let ok = |v: f64| v.is_finite();
    g.panels.iter().flat_map(|p| &p.items).find(|i| !match i {
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

fn histogram(binning: Value, values: &[Value]) -> Geometry {
    let rows: Vec<Value> = values.iter().map(|v| json!({"v": v})).collect();
    draw(
        json!({"type": "histogram", "title": "T", "encoding": {"x": "v"}, "binning": binning}),
        &rows,
    )
}

fn counts(g: &Geometry) -> Vec<String> {
    observations(g)
        .iter()
        .map(|o| line(o, "Count").unwrap_or("?").to_owned())
        .collect()
}

#[test]
fn histogram_bins_are_left_closed_and_the_last_is_closed() {
    let g = histogram(
        json!({"edges": [0, 1, 3]}),
        &[json!("0.00"), json!("0.500"), json!("1.00"), json!("3.000")],
    );
    assert!(g.failure.is_none());
    assert_eq!(counts(&g), ["2", "2"], "[0, 1) holds 0 and 0.5; [1, 3] holds 1 and 3");
    let binning = check(&g, "binning").expect("binning is disclosed");
    assert_eq!(binning.details["edges"], json!([0.0, 1.0, 3.0]));
}

#[test]
fn histogram_discloses_missing_and_outside_values_instead_of_counting_them() {
    let g = histogram(
        json!({"edges": [0, 1, 3]}),
        &[
            json!(-1),
            Value::Null,
            json!("NA"),
            json!(0),
            json!(1),
            json!(3),
            json!(4),
        ],
    );
    assert!(g.failure.is_none());
    assert_eq!(counts(&g), ["1", "2"]);
    let excluded = check(&g, "excluded-observations").expect("exclusions are disclosed");
    assert_eq!(excluded.details, json!({"missing": 2, "outside": 2}));
    assert!(g.note.as_deref().unwrap_or("").contains("4 not counted"));
}

#[test]
fn histogram_text_that_is_not_a_number_is_an_error_with_its_row() {
    let g = histogram(json!({"edges": [0, 1, 3]}), &[json!(0), json!("wrong")]);
    assert!(g.failure.is_some(), "a value that is not a number never becomes a bin");
    let c = check(&g, "non-numeric").expect("named");
    assert!(c.message.contains("Row 2"), "{}", c.message);
}

#[test]
fn histogram_density_gives_equal_counts_equal_area() {
    let g = histogram(
        json!({"edges": [0, 1, 3], "measure": "density"}),
        &[json!(0.5), json!(2)],
    );
    let areas: Vec<f64> = marks(&g)
        .iter()
        .filter_map(|i| match i {
            Item::Rect { rect, .. } => Some(rect.w * rect.h),
            _ => None,
        })
        .collect();
    assert_eq!(areas.len(), 2);
    assert!(
        (areas[0] - areas[1]).abs() / areas[0] < 0.005,
        "a bin twice as wide is half as tall: {areas:?}"
    );
}

#[test]
fn histogram_extremes_and_constants_keep_finite_bins() {
    let huge = histogram(json!({"count": 4}), &[json!(-1e308), json!(1e308)]);
    assert!(huge.failure.is_none() && finite(&huge));
    assert_eq!(counts(&huge), ["1", "0", "0", "1"]);
    let tiny = histogram(
        json!({"count": 2, "measure": "density"}),
        &[json!(1e-308), json!(2e-308)],
    );
    assert!(tiny.failure.is_none() && finite(&tiny));
    assert_eq!(observations(&tiny).len(), 2);
    let constant = histogram(json!({}), &[json!(7), json!(7)]);
    assert!(constant.failure.is_none() && finite(&constant));
    assert_eq!(counts(&constant), ["2"], "a constant sample is one bin");
}

fn boxplot(values: &[Value], extra: Value) -> Geometry {
    let rows: Vec<Value> = values.iter().map(|v| json!({"g": "A", "v": v})).collect();
    let mut spec = json!({"type": "boxplot", "title": "T", "encoding": {"x": "g", "y": "v"}});
    if let (Some(s), Some(e)) = (spec.as_object_mut(), extra.as_object()) {
        s.extend(e.clone());
    }
    draw(spec, &rows)
}

fn outliers(g: &Geometry) -> usize {
    marks(g).iter().filter(|i| matches!(i, Item::Point { .. })).count()
}

#[test]
fn box_quartiles_follow_r7() {
    let g = boxplot(&[json!(0), json!(10), json!(30)], json!({}));
    let summary = observations(&g)
        .into_iter()
        .find(|o| line(o, "Median").is_some())
        .expect("a box readout");
    assert_eq!(line(summary, "First quartile"), Some("5"));
    assert_eq!(line(summary, "Median"), Some("10"));
    assert_eq!(line(summary, "Third quartile"), Some("20"));
}

#[test]
fn box_outliers_lie_beyond_one_and_a_half_iqr_in_both_orientations() {
    let sample: Vec<Value> = [100, 1, 2, 3, 4, -100, 5, 6, 7]
        .iter()
        .map(|v| json!(v.to_string()))
        .collect();
    for extra in [json!({}), json!({"style": {"orientation": "horizontal"}})] {
        let g = boxplot(&sample, extra.clone());
        assert_eq!(outliers(&g), 2, "{extra}: 100 and -100");
        let rows: Vec<String> = observations(&g)
            .iter()
            .filter(|o| o.readout.heading.contains("· row"))
            .map(|o| o.readout.heading.clone())
            .collect();
        assert!(
            rows.iter().any(|h| h.ends_with("row 1")) && rows.iter().any(|h| h.ends_with("row 6")),
            "outliers name their source rows: {rows:?}"
        );
    }
}

#[test]
fn box_constant_and_extreme_samples_stay_finite() {
    let constant = boxplot(&[json!(7), json!(7)], json!({}));
    assert!(constant.failure.is_none() && finite(&constant));
    assert_eq!(outliers(&constant), 0);
    let box_height = marks(&constant)
        .iter()
        .find_map(|i| match i {
            Item::Rect { rect, .. } => Some(rect.h),
            _ => None,
        })
        .unwrap_or(0.0);
    assert!(box_height < 1.0, "no false box height: {box_height}");
    for sample in [
        vec![json!(-1e308), json!(0), json!(1e308)],
        vec![json!(1e-308), json!(2e-308), json!(3e-308)],
        vec![json!(f64::MAX)],
    ] {
        let g = boxplot(&sample, json!({}));
        assert!(g.failure.is_none() && finite(&g), "{sample:?}");
        assert_eq!(outliers(&g), 0, "{sample:?}");
    }
}

fn intervals(rows: &[Value], extra: Value) -> Geometry {
    let mut spec =
        json!({"type": "interval", "title": "T", "encoding": {"x": "c", "lower": "lo", "upper": "hi", "y": "e"}});
    if let (Some(s), Some(e)) = (spec.as_object_mut(), extra.as_object()) {
        s.extend(e.clone());
    }
    draw(spec, rows)
}

/// Short horizontal mark lines: the caps at range ends.
fn caps(g: &Geometry) -> usize {
    marks(g)
        .iter()
        .filter(|i| match i {
            Item::Line { path, .. } => match path.cmds.as_slice() {
                [Cmd::M { p: a }, Cmd::L { p: b }] => (a[1] - b[1]).abs() < 1e-9,
                _ => false,
            },
            _ => false,
        })
        .count()
}

#[test]
fn an_interval_without_an_estimate_never_shows_a_midpoint() {
    let g = intervals(
        &[
            json!({"c": "First", "lo": 2, "hi": 8, "e": 5}),
            json!({"c": "Second", "lo": 3, "hi": 6, "e": null}),
        ],
        json!({}),
    );
    assert!(g.failure.is_none());
    let estimates = marks(&g).iter().filter(|i| matches!(i, Item::Point { .. })).count();
    assert_eq!(estimates, 1);
    let second = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading.contains("Second"))
        .expect("second");
    assert_eq!(line(second, "Lower"), Some("3"));
    assert_eq!(line(second, "Upper"), Some("6"));
    assert_eq!(second.readout.lines.len(), 2, "no estimate line");
    assert!(!second.readout.lines.iter().any(|l| l.value.contains("4.5")));
}

#[test]
fn a_clipped_interval_end_has_no_cap() {
    let g = intervals(
        &[
            json!({"c": "Cut", "lo": -5, "hi": 15, "e": 5}),
            json!({"c": "Whole", "lo": 2, "hi": 8, "e": 7}),
        ],
        json!({"axes": {"y": {"min": 0, "max": 10}}}),
    );
    assert!(g.failure.is_none());
    assert_eq!(caps(&g), 2, "only the whole range has caps");
}

fn line_chart(kind: &str, curve: &str, x: &[Value], values: &[f64]) -> Geometry {
    let rows: Vec<Value> = x.iter().zip(values).map(|(x, v)| json!({"x": x, "v": v})).collect();
    draw(
        json!({"type": kind, "title": "T", "encoding": {"x": "x", "y": "v"}, "style": {"curve": curve}}),
        &rows,
    )
}

/// The polylines of a chart's line and area marks.
fn mark_points(g: &Geometry) -> Vec<Vec<[f64; 2]>> {
    marks(g)
        .iter()
        .filter_map(|i| match i {
            Item::Line { path, .. } | Item::Area { path, .. } => Some(path),
            _ => None,
        })
        .map(|p| {
            p.cmds
                .iter()
                .filter_map(|c| match c {
                    Cmd::M { p } | Cmd::L { p } => Some(*p),
                    Cmd::C { p, .. } => Some(*p),
                    Cmd::Z => None,
                })
                .collect()
        })
        .collect()
}

#[test]
fn step_curves_hold_values_with_axis_aligned_segments() {
    let axes: [Vec<Value>; 3] = [
        vec![json!(0), json!(5), json!(10)],
        vec![json!("First"), json!("Middle"), json!("Last")],
        vec![json!("2024-01-01"), json!("2024-02-01"), json!("2024-03-01")],
    ];
    for kind in ["line", "area"] {
        for curve in ["step", "step_before", "step_after"] {
            for x in &axes {
                let g = line_chart(kind, curve, x, &[2.0, -3.0, 7.0]);
                assert!(g.failure.is_none(), "{kind} {curve} {x:?}");
                let paths = marks(&g);
                assert!(
                    paths.iter().all(|i| match i {
                        Item::Line { path, .. } | Item::Area { path, .. } =>
                            !path.cmds.iter().any(|c| matches!(c, Cmd::C { .. })),
                        _ => true,
                    }),
                    "{kind} {curve}: no curves"
                );
                for points in mark_points(&g) {
                    for w in points.windows(2) {
                        let [a, b] = [w[0], w[1]];
                        assert!(
                            (a[0] - b[0]).abs() < 1e-6 || (a[1] - b[1]).abs() < 1e-6,
                            "{kind} {curve} {x:?}: a sloped segment {a:?} → {b:?}"
                        );
                    }
                }
                assert_eq!(observations(&g).len(), 3, "steps invent no observations");
            }
        }
    }
}

#[test]
fn monotone_smoothing_never_overshoots_and_linear_draws_straight() {
    let x: Vec<Value> = [0, 2, 7, 13, 20].iter().map(|v| json!(v)).collect();
    let values = [2.0, 9.25, 4.125, 4.125, 11.5];
    for kind in ["line", "area"] {
        // The style default is full monotone.
        let rows: Vec<Value> = x.iter().zip(values).map(|(x, v)| json!({"x": x, "v": v})).collect();
        let default = draw(
            json!({"type": kind, "title": "T", "encoding": {"x": "x", "y": "v"}}),
            &rows,
        );
        let monotone = line_chart(kind, "monotone", &x, &values);
        assert_eq!(marks(&default), marks(&monotone), "{kind}: monotone is the default");
        let mut curves = 0;
        for item in marks(&monotone) {
            let (Item::Line { path, .. } | Item::Area { path, .. }) = item else {
                continue;
            };
            let mut at = [0.0, 0.0];
            for c in &path.cmds {
                match c {
                    Cmd::M { p } | Cmd::L { p } => at = *p,
                    Cmd::C { c1, c2, p } => {
                        curves += 1;
                        let (lo, hi) = (at[1].min(p[1]), at[1].max(p[1]));
                        for k in 0..=100 {
                            let t = k as f64 / 100.0;
                            let u = 1.0 - t;
                            let y = u * u * u * at[1]
                                + 3.0 * u * u * t * c1[1]
                                + 3.0 * u * t * t * c2[1]
                                + t * t * t * p[1];
                            assert!(
                                y >= lo - 1e-3 && y <= hi + 1e-3,
                                "{kind}: the curve overshoots its observations: {y} outside {lo}..{hi}"
                            );
                        }
                        at = *p;
                    }
                    Cmd::Z => {}
                }
            }
        }
        assert!(curves > 0, "{kind}: monotone draws curves");
        let linear = line_chart(kind, "linear", &x, &values);
        assert!(
            marks(&linear).iter().all(|i| match i {
                Item::Line { path, .. } | Item::Area { path, .. } =>
                    !path.cmds.iter().any(|c| matches!(c, Cmd::C { .. })),
                _ => true,
            }),
            "{kind}: linear draws straight segments"
        );
    }
}

#[test]
fn a_tiny_bar_keeps_a_pixel_of_ink_and_a_zero_bar_none() {
    let rows = [
        json!({"c": "Positive", "v": "12.000"}),
        json!({"c": "Negative", "v": "-8.000"}),
        json!({"c": "Tiny", "v": "0.0001"}),
        json!({"c": "Zero", "v": "-0.000"}),
    ];
    let g = draw(
        json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}}),
        &rows,
    );
    for (width, horizontal) in [(640.0, false), (640.0, true)] {
        let g = draw_with(
            json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"},
                "style": {"orientation": if horizontal { "horizontal" } else { "vertical" }}}),
            &rows,
            &ChartTheme::default(),
            Options {
                width,
                ..Options::default()
            },
        );
        let bars: Vec<f64> = marks(&g)
            .iter()
            .filter_map(|i| match i {
                Item::Rect { rect, .. } => Some(if horizontal { rect.w } else { rect.h }),
                _ => None,
            })
            .collect();
        assert_eq!(
            bars.len(),
            3,
            "horizontal {horizontal}: the zero bar draws nothing: {bars:?}"
        );
        assert!(
            bars.iter().all(|l| *l >= 1.0 - 1e-6),
            "horizontal {horizontal}: every non-zero bar keeps a pixel: {bars:?}"
        );
    }
    assert_eq!(observations(&g).len(), 4, "the zero bar still reads");
    assert!(
        g.panels[0].items.iter().any(|i| i.role() == Role::Baseline),
        "signed bars share a baseline at zero"
    );
}

#[test]
fn zoom_windows_stay_finite_increasing_and_inside_the_data_at_every_scale() {
    let scales: [(&str, Vec<f64>); 5] = [
        ("ordinary", vec![-100.0, 0.0, 100.0]),
        ("huge", vec![-1e308, 0.0, 1e308]),
        ("tiny", vec![-1e-308, 0.0, 1e-308]),
        ("subnormal", vec![-f64::from_bits(1), 0.0, f64::from_bits(1)]),
        ("adjacent", vec![1.0, 1.0 + f64::EPSILON]),
    ];
    for kind in ["line", "area", "scatter"] {
        for (name, xs) in &scales {
            let rows: Vec<Value> = xs
                .iter()
                .enumerate()
                .map(|(i, x)| json!({"x": x, "v": i + 1}))
                .collect();
            let spec = json!({"type": kind, "title": "T", "encoding": {"x": "x", "y": "v"}});
            let (lo, hi) = (xs[0], xs[xs.len() - 1]);
            let windows = [
                None,
                Some([lo + (hi - lo) * 0.25, hi - (hi - lo) * 0.25]),
                Some([lo / 2.0 + hi / 2.0, hi]),
                // Beyond the data on both sides: clamped to it.
                Some([f64::MIN, f64::MAX]),
            ];
            for zoom in windows {
                let g = draw_with(
                    spec.clone(),
                    &rows,
                    &ChartTheme::default(),
                    Options {
                        zoom,
                        ..Options::default()
                    },
                );
                assert!(g.failure.is_none(), "{kind} {name} {zoom:?}");
                assert!(finite(&g), "{kind} {name} {zoom:?}: finite geometry");
                let x = g.panels[0].x.as_ref().expect("a numeric x scale");
                let [a, b] = x.domain;
                assert!(
                    a.is_finite() && b.is_finite() && a < b,
                    "{kind} {name} {zoom:?}: {a} < {b}"
                );
                // The zoom limit covers the data; scatter plots pad it so edge points draw whole.
                let [e0, e1] = x.extent;
                assert!(
                    e0 <= lo && e1 >= hi,
                    "{kind} {name}: the extent {e0}..{e1} covers the data"
                );
                assert!(a >= e0 && b <= e1, "{kind} {name} {zoom:?}: {a}..{b} inside {e0}..{e1}");
                if kind != "scatter" {
                    assert!(a >= lo && b <= hi, "{kind} {name} {zoom:?}: {a}..{b} inside {lo}..{hi}");
                }
                for o in observations(&g) {
                    assert!(o.x.is_finite(), "{kind} {name}: finite readout anchor");
                }
            }
        }
    }
}

#[test]
fn a_value_that_underflows_is_an_error_at_its_row_and_never_a_zero() {
    for (kind, channel) in [
        ("line", "y"),
        ("area", "y"),
        ("scatter", "y"),
        ("scatter", "x"),
        ("bar", "y"),
    ] {
        let rows = [json!({"x": 1, "v": 2}), json!({"x": 2, "v": 3})];
        let mut rows = rows.to_vec();
        let field = if channel == "x" { "x" } else { "v" };
        rows[1][field] = json!("1e-400");
        let g = draw(
            json!({"type": kind, "title": "T", "encoding": {"x": "x", "y": "v"}}),
            &rows,
        );
        let errors: Vec<_> = g
            .checks
            .iter()
            .filter(|c| c.level == richview_chart::diag::Level::Error)
            .collect();
        assert!(!errors.is_empty(), "{kind} {channel}: 1e-400 is reported");
        assert!(
            errors
                .iter()
                .any(|c| c.path.as_deref().is_some_and(|p| p.contains('1'))),
            "{kind} {channel}: the error names row 2: {errors:?}"
        );
        assert!(
            !observations(&g)
                .iter()
                .any(|o| o.readout.lines.iter().any(|l| l.value == "0")),
            "{kind} {channel}: never drawn as zero"
        );
    }
}

/// A heatmap row named by one long word keeps its name on the chart at phone width: the
/// gutter widens for an unbreakable name instead of letting it run off the left edge.
#[test]
fn a_long_single_word_heatmap_row_name_stays_inside_the_chart() {
    let rows: Vec<Value> = ["Superintelligence", "Math"]
        .iter()
        .flat_map(|model| {
            ["Jan", "Feb", "Mar"]
                .iter()
                .enumerate()
                .map(move |(i, month)| json!({"model": model, "month": month, "score": i}))
        })
        .collect();
    let spec = json!({
        "type": "heatmap",
        "title": "Scores",
        "encoding": {"x": "month", "y": "model", "color": "score"}
    });
    for width in [272.0, 320.0] {
        let g = draw_with(
            spec.clone(),
            &rows,
            &ChartTheme::default(),
            Options {
                width,
                ..Options::default()
            },
        );
        assert!(check(&g, "clipping").is_none(), "{width}: {:?}", g.checks);
        let name = g
            .panels
            .iter()
            .flat_map(|p| &p.items)
            .find_map(|i| match i {
                Item::Text(t) if t.text == "Superintelligence" => Some(t.bounds),
                _ => None,
            })
            .expect("the row is named");
        assert!(name.x >= -0.5, "{width}: {name:?}");
    }
}

fn texts_of(g: &Geometry) -> Vec<&richview_chart::geometry::Text> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn soft_ink(theme: &ChartTheme) -> richview_chart::geometry::Rgba {
    richview_chart::geometry::Rgba::from_oklch(richview_chart::color::parse(&theme.ink_soft).expect("ink-soft"), 1.0)
}

/// STY-39: ticks, category labels and axis titles are regular weight in ink-soft; only data
/// labels take the primary ink.
#[test]
fn axis_text_is_regular_weight_and_ink_soft_and_data_labels_keep_the_ink() {
    let theme = ChartTheme::default();
    let soft = soft_ink(&theme);
    let rows: Vec<Value> = [("North", 4.0), ("South", 9.0), ("East", 6.0)]
        .iter()
        .map(|(c, v)| json!({"c": c, "v": v}))
        .collect();
    let specs = [
        json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"},
               "axes": {"x": {"label": "Region"}, "y": {"label": "Sales"}}}),
        json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}, "style": {"orientation": "horizontal"},
               "axes": {"x": {"label": "Region"}, "y": {"label": "Sales"}}}),
        json!({"type": "line", "title": "T", "encoding": {"x": "c", "y": "v"},
               "axes": {"y": {"label": "Sales"}}}),
    ];
    for spec in specs {
        let g = draw_with(spec.clone(), &rows, &theme, Options::default());
        let furniture: Vec<_> = texts_of(&g)
            .into_iter()
            .filter(|t| matches!(t.role, Role::Tick | Role::AxisLabel | Role::AxisTitle))
            .collect();
        assert!(furniture.len() >= 4, "{spec}: axis text is drawn");
        for t in furniture {
            assert_eq!(t.weight, 400, "{spec}: {:?} is regular weight", t.text);
            assert_eq!(t.color, soft, "{spec}: {:?} is ink-soft", t.text);
        }
    }
    let bridge = [
        json!({"step": "Revenue", "amount": 120, "kind": "start"}),
        json!({"step": "Costs", "amount": -45}),
        json!({"step": "Profit", "amount": 75, "kind": "total"}),
    ];
    let g = draw_with(
        json!({"type": "waterfall", "title": "T", "encoding": {"x": "step", "y": "amount", "measure": "kind"}}),
        &bridge,
        &theme,
        Options::default(),
    );
    let ink = richview_chart::geometry::Rgba::from_oklch(richview_chart::color::parse(&theme.ink).unwrap(), 1.0);
    let labels: Vec<_> = texts_of(&g).into_iter().filter(|t| t.role == Role::Label).collect();
    assert!(!labels.is_empty(), "the bridge carries value labels");
    for t in labels {
        assert_eq!(t.color, ink, "data label {:?} keeps the ink", t.text);
        assert_eq!(t.weight, 400);
    }
}

/// Overlapping areas stay in their own hues: translucent fills, all under every line.
#[test]
fn area_fills_are_translucent_in_the_series_hue_and_sit_under_every_line() {
    let rows: Vec<Value> = (0..6)
        .flat_map(|i| {
            [
                json!({"t": i, "v": 10 + i, "s": "A"}),
                json!({"t": i, "v": 14 - i, "s": "B"}),
            ]
        })
        .collect();
    let g = draw(
        json!({"type": "area", "title": "T", "encoding": {"x": "t", "y": "v", "color": "s"}}),
        &rows,
    );
    let items: Vec<&Item> = g.panels.iter().flat_map(|p| &p.items).collect();
    let fills: Vec<usize> = (0..items.len())
        .filter(|&i| matches!(items[i], Item::Area { role: Role::Mark, .. }))
        .collect();
    let lines: Vec<usize> = (0..items.len())
        .filter(|&i| matches!(items[i], Item::Line { role: Role::Mark, .. }))
        .collect();
    assert_eq!(fills.len(), 2);
    assert!(!lines.is_empty());
    assert!(
        fills.iter().max() < lines.iter().min(),
        "every fill precedes every line"
    );
    for &f in &fills {
        let Item::Area { fill, series, .. } = items[f] else {
            unreachable!()
        };
        assert!(fill.color.0[3] <= 0.3, "a translucent fill: {}", fill.color.0[3]);
        let own = lines
            .iter()
            .find_map(|&l| match items[l] {
                Item::Line { stroke, series: s, .. } if s == series => Some(stroke.color),
                _ => None,
            })
            .expect("the series has a line");
        assert_eq!(fill.color.0[..3], own.0[..3], "the fill is in the line's own hue");
    }
}

/// Pie and ring slices are parted by a surface-coloured gap, never an ink stroke.
#[test]
fn pie_and_ring_slices_are_separated_by_a_surface_gap() {
    let theme = ChartTheme::default();
    let surface =
        richview_chart::geometry::Rgba::from_oklch(richview_chart::color::parse(&theme.surface).unwrap(), 1.0);
    let rows = vec![
        json!({"c": "A", "v": 5}),
        json!({"c": "B", "v": 3}),
        json!({"c": "C", "v": 2}),
    ];
    for inner in [0.0, 0.5] {
        let g = draw_with(
            json!({"type": "pie", "title": "T", "encoding": {"x": "c", "y": "v"}, "style": {"inner": inner}}),
            &rows,
            &theme,
            Options::default(),
        );
        let seams: Vec<_> = g
            .panels
            .iter()
            .flat_map(|p| &p.items)
            .filter_map(|i| match i {
                Item::Line {
                    stroke,
                    role: Role::Mark,
                    ..
                } => Some(stroke),
                _ => None,
            })
            .collect();
        assert_eq!(seams.len(), 3, "inner {inner}: a seam per slice");
        for s in seams {
            assert_eq!(s.color, surface, "inner {inner}: seams take the surface colour");
            assert!(s.width >= 2.0, "inner {inner}: a visible gap, not a hairline");
        }
    }
}

/// The bubble size key shows the smallest, middle and largest bubble with formatted values and
/// a title that is the axis label or a readable name, never the raw field.
#[test]
fn the_bubble_size_key_has_three_circles_with_values_and_a_readable_title() {
    let rows: Vec<Value> = [(1, 100.0), (2, 300.0), (3, 1500.0)]
        .iter()
        .map(|(x, acv)| json!({"x": x, "y": x, "acv": acv}))
        .collect();
    let key = |spec: Value| {
        let g = draw(spec, &rows);
        let circles: Vec<f64> = g
            .figure_items
            .iter()
            .filter_map(|i| match i {
                Item::Point {
                    radius,
                    role: Role::Key,
                    ..
                } => Some(*radius),
                _ => None,
            })
            .collect();
        let words: Vec<String> = g
            .figure_items
            .iter()
            .filter_map(|i| match i {
                Item::Text(t) if t.role == Role::Key => Some(t.text.clone()),
                _ => None,
            })
            .collect();
        (circles, words)
    };
    let (circles, words) =
        key(json!({"type": "scatter", "title": "T", "encoding": {"x": "x", "y": "y", "size": "acv"}}));
    assert_eq!(circles.len(), 3, "min, mid and max circles");
    assert!(circles[0] < circles[1] && circles[1] < circles[2]);
    assert_eq!(words.len(), 4, "a title and three values: {words:?}");
    assert_eq!(words[0], "Acv", "the field read as a name, not raw");
    assert!(
        words[1..].iter().all(|w| w.starts_with(|c: char| c.is_ascii_digit())),
        "{words:?}"
    );
    assert_ne!(words[1], words[3]);
    let (_, words) = key(
        json!({"type": "scatter", "title": "T", "encoding": {"x": "x", "y": "y", "size": "acv"},
        "axes": {"size": {"label": "Annual contract value", "unit": "k€"}}}),
    );
    assert_eq!(words[0], "Annual contract value");
    assert!(words[1..].iter().all(|w| w.contains("k€")), "{words:?}");
}

/// A key whose largest value takes a scale word writes its smallest one the same way: "4,607"
/// beside "715 million" read as a stray number (Daniel, 15 Nov 2026).
#[test]
fn the_bubble_size_key_writes_its_three_values_one_way() {
    let words = |sizes: [f64; 3]| -> Vec<String> {
        let rows: Vec<Value> = sizes
            .iter()
            .enumerate()
            .map(|(i, s)| json!({"x": i, "y": i, "pop": s}))
            .collect();
        let g = draw(
            json!({"type": "scatter", "title": "T", "encoding": {"x": "x", "y": "y", "size": "pop"}}),
            &rows,
        );
        g.figure_items
            .iter()
            .filter_map(|i| match i {
                Item::Text(t) if t.role == Role::Key => Some(t.text.clone()),
                _ => None,
            })
            .skip(1)
            .collect()
    };
    assert_eq!(
        words([4607.0, 5.0e8, 1.43e9]),
        ["4.61 thousand", "715 million", "1.43 billion"]
    );
    // Below 1,000 there is no scale word to take, and a key of small values stays plain.
    assert_eq!(words([607.0, 2.0e6, 4.0e6])[0], "607");
    assert_eq!(words([4607.0, 5000.0, 9000.0]), ["4,607", "6,804", "9,000"]);
}

#[test]
fn bubble_reference_midpoints_stay_finite_at_the_numeric_limit() {
    let g = draw(
        json!({"type":"scatter","title":"T","encoding":{"x":"x","y":"y","size":"size"}}),
        &[json!({"x":1,"y":1,"size":1e308}), json!({"x":2,"y":2,"size":f64::MAX})],
    );
    let circles: Vec<_> = g
        .figure_items
        .iter()
        .filter_map(|item| match item {
            Item::Point {
                radius,
                center,
                role: Role::Key,
                ..
            } => Some((*radius, *center)),
            _ => None,
        })
        .collect();
    assert_eq!(circles.len(), 3);
    assert!(
        circles
            .iter()
            .all(|(r, c)| r.is_finite() && c.iter().all(|v| v.is_finite()))
    );
    assert!(
        !g.figure_items
            .iter()
            .any(|item| matches!(item,Item::Text(t) if t.role==Role::Key&&t.text=="n/a"))
    );
}

#[test]
fn gapminder_size_key_midpoints_use_each_regions_complete_playback_range() {
    use richview_chart::reader_controls::Controls;
    let chart = json!({"type":"scatter","title":"T","encoding":{"x":"income","y":"life","size":"pop","color":"region"},
        "controls":[{"kind":"playback","id":"year","field":"year","initial":"2000"},
        {"kind":"filter","id":"region","field":"region","options":["Europe","Asia"],"initial":"Europe"}]});
    let source = include_str!("../../examples/playable-gapminder/data/gapminder.csv");
    let rows: Vec<Value> = source
        .lines()
        .skip(1)
        .map(|line| {
            let cells: Vec<_> = line.split(',').collect();
            json!({"year":cells[0],"region":cells[1],"income":cells[3].parse::<f64>().unwrap(),
            "life":cells[4].parse::<f64>().unwrap(),"pop":cells[5].parse::<f64>().unwrap()})
        })
        .collect();
    let controls = Controls::parse(&chart, &rows);
    for (region, expected) in [
        (0, ["30 million", "56.5 million", "83 million"]),
        (1, ["25 million", "718 million", "1.41 billion"]),
    ] {
        for frame in 0..controls.list[0].options.len() {
            let state = vec![frame, region];
            let resolved = controls.chart(&chart, &rows, &state).unwrap();
            let theme = ChartTheme::default();
            let options = controls.options(
                &chart,
                &rows,
                &state,
                &theme,
                &Options {
                    width: 1100.0,
                    ..Default::default()
                },
            );
            let g = layout(&resolved.block, &resolved.rows, &theme, &options);
            let labels: Vec<_> = g
                .figure_items
                .iter()
                .filter_map(|item| match item {
                    Item::Text(t) if t.role == Role::Key => Some(t.text.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(&labels[1..], &expected, "region {region}, frame {frame}");
        }
    }
}

fn at_width(spec: Value, rows: &[Value], width: f64) -> Geometry {
    draw_with(
        spec,
        rows,
        &ChartTheme::default(),
        Options {
            width,
            ..Options::default()
        },
    )
}

fn every_text(g: &Geometry) -> Vec<&richview_chart::geometry::Text> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .chain(&g.figure_items)
        .filter_map(|i| match i {
            Item::Text(t) => Some(t),
            _ => None,
        })
        .collect()
}

/// On a phone, a category name above its bar keeps six pixels from the axis line.
#[test]
fn horizontal_bar_names_keep_six_pixels_from_the_axis_line() {
    let rows: Vec<Value> = [("North America", 40), ("Europe", 25), ("APAC", 31)]
        .iter()
        .map(|(c, v)| json!({"c": c, "v": v}))
        .collect();
    for width in [320.0, 390.0] {
        let g = at_width(
            json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}, "style": {"orientation": "horizontal"}}),
            &rows,
            width,
        );
        let axis_x = marks(&g)
            .iter()
            .filter_map(|i| match i {
                Item::Rect { rect, .. } => Some(rect.x),
                _ => None,
            })
            .fold(f64::INFINITY, f64::min);
        let names: Vec<_> = every_text(&g)
            .into_iter()
            .filter(|t| t.role == Role::AxisLabel && rows.iter().any(|r| r["c"] == t.text.as_str()))
            .collect();
        assert_eq!(names.len(), 3, "{width}: every name is drawn");
        for t in names {
            assert!(
                t.bounds.x - axis_x >= 5.0,
                "{width}: {:?} at {}",
                t.text,
                t.bounds.x - axis_x
            );
        }
    }
}

/// Text stays inside the figure and 12 px above its bottom edge, so a caption below never touches
/// the axis labels; a line's plot runs to the figure's right edge.
#[test]
fn text_stays_inside_the_figure_with_room_below_and_the_plot_fills_the_width() {
    let rows: Vec<Value> = (0..24)
        .map(|m| json!({"t": format!("{}-{:02}-01", 2024 + m / 12, m % 12 + 1), "v": 10 + m * 3}))
        .collect();
    for kind in ["line", "area"] {
        for width in [320.0, 640.0, 1440.0] {
            let g = at_width(
                json!({"type": kind, "title": "T", "encoding": {"x": "t", "y": "v"}, "axes": {"x": {"label": "Month"}, "y": {"label": "Sales"}}}),
                &rows,
                width,
            );
            let texts = every_text(&g);
            for t in &texts {
                if t.rotate == 0.0 {
                    assert!(
                        t.bounds.x >= -1e-6 && t.bounds.right() <= width + 1e-6,
                        "{kind} {width}: {t:?}"
                    );
                }
            }
            let lowest = texts.iter().map(|t| t.bounds.bottom()).fold(0.0, f64::max);
            assert!(
                g.height - lowest >= 12.0 - 1e-6,
                "{kind} {width}: {} below the text",
                g.height - lowest
            );
            let reach = g
                .panels
                .iter()
                .flat_map(|p| &p.items)
                .filter_map(|i| match i {
                    Item::Line {
                        path, role: Role::Mark, ..
                    } => Some(path),
                    _ => None,
                })
                .flat_map(|p| p.cmds.iter())
                .filter_map(|c| match c {
                    Cmd::M { p } | Cmd::L { p } | Cmd::C { p, .. } => Some(p[0]),
                    Cmd::Z => None,
                })
                .fold(0.0, f64::max);
            assert!(
                (reach - (width - 9.0)).abs() <= 1e-6,
                "{kind} {width}: the line ends at {reach}; the endpoint halo needs 9px"
            );
        }
    }
}

fn at_width_themed(spec: Value, rows: &[Value], theme: &ChartTheme, width: f64) -> Geometry {
    draw_with(
        spec,
        rows,
        theme,
        Options {
            width,
            ..Options::default()
        },
    )
}

fn decal_theme() -> ChartTheme {
    ChartTheme {
        decals: true,
        ..ChartTheme::default()
    }
}

/// A hatch's pitch grows as its mark shrinks, so a thin pie slice or a small bar is never solid;
/// large marks keep the base pitch.
#[test]
fn hatch_pitch_grows_as_the_mark_shrinks() {
    let rows = vec![
        json!({"c": "Big", "v": 94}),
        json!({"c": "Mid", "v": 5}),
        json!({"c": "Thin", "v": 1}),
    ];
    let g = draw_with(
        json!({"type": "pie", "title": "T", "encoding": {"x": "c", "y": "v"}}),
        &rows,
        &decal_theme(),
        Options::default(),
    );
    let scales: Vec<f64> = g
        .panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Area {
                fill, role: Role::Mark, ..
            } if fill.hatch.is_some() => Some(fill.hatch_scale),
            _ => None,
        })
        .collect();
    assert_eq!(scales.len(), 3, "every slice is hatched");
    assert_eq!(scales[0], 1.0, "the big slice keeps the base pitch");
    assert!(scales[2] > scales[1] && scales[1] >= 1.0, "{scales:?}");
    assert!(scales[2] > 1.5 && scales[2] <= 2.5, "{scales:?}");
    let bars = draw_with(
        json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}}),
        &(0..40)
            .map(|i| json!({"c": format!("c{i}"), "v": i + 1}))
            .collect::<Vec<_>>(),
        &decal_theme(),
        Options::default(),
    );
    let coarse = marks(&bars)
        .iter()
        .filter(|i| matches!(i, Item::Rect { fill, .. } if fill.hatch_scale > 1.0))
        .count();
    assert!(coarse > 0, "narrow bars take a coarser pitch");
}

/// A value label over a hatch sits on a surface-coloured box that covers it.
#[test]
fn labels_over_a_hatch_sit_on_a_surface_box() {
    use richview_chart::color::{self, Oklch};
    use richview_chart::geometry::Rgba;
    let over = |a: Rgba, b: Rgba| {
        let alpha = a.0[3];
        Rgba([
            a.0[0] * alpha + b.0[0] * (1.0 - alpha),
            a.0[1] * alpha + b.0[1] * (1.0 - alpha),
            a.0[2] * alpha + b.0[2] * (1.0 - alpha),
            1.0,
        ])
    };
    let oklch = |c: Rgba| Oklch::from_srgb([f64::from(c.0[0]), f64::from(c.0[1]), f64::from(c.0[2])]);
    let v: Value = serde_json::from_str(include_str!("fixtures/types/streamgraph.json")).unwrap();
    let rows = v["rows"].as_array().unwrap().clone();
    let theme = decal_theme();
    let surface = Rgba::from_oklch(color::parse(&theme.surface).unwrap(), 1.0);
    let g = at_width_themed(v["spec"].clone(), &rows, &theme, 1440.0);
    let items: Vec<&Item> = g.panels.iter().flat_map(|p| &p.items).collect();
    let hatched: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            Item::Area { fill, .. } | Item::Rect { fill, .. } if fill.hatch.is_some() => Some(fill),
            _ => None,
        })
        .collect();
    assert!(!hatched.is_empty());
    let labels: Vec<usize> = (0..items.len())
        .filter(|&i| matches!(items[i], Item::Text(t) if t.role == Role::Label && ["Rock", "Jazz", "Pop"].contains(&t.text.as_str())))
        .collect();
    assert!(!labels.is_empty(), "the streams are named");
    for i in labels {
        let Item::Text(t) = items[i] else { unreachable!() };
        let Item::Rect { rect, fill, role, .. } = items[i - 1] else {
            panic!("{:?} has no box under it", t.text);
        };
        assert_eq!(*role, Role::Annotation);
        assert!(fill.hatch.is_none() && fill.color.0[3] > 0.8);
        assert!(t.bounds.inside(rect, 0.01), "{:?} sits inside its box", t.text);
        // The halo changes the background the glyph is painted on. Check both the mark's
        // fill and its hatch ink underneath, including their actual alpha composition.
        for mark in &hatched {
            for base in [mark.color, mark.hatch_ink.unwrap_or(mark.color)] {
                let background = over(fill.color, over(base, surface));
                let ratio = color::contrast(oklch(over(t.color, background)), oklch(background));
                assert!(
                    ratio >= 4.5,
                    "{} has final halo contrast {ratio}: {:?} on {:?}",
                    t.text,
                    t.color,
                    background
                );
            }
        }
    }
}

/// With the names in a legend instead of at the line ends, a multi-series line or area plot runs
/// to the endpoint halo's 9px inset: the last x label leans in rather than narrowing the plot.
#[test]
fn multi_series_plots_fill_the_width_when_the_names_sit_in_a_legend() {
    let rows: Vec<Value> = (0..12)
        .flat_map(|m| {
            let t = format!("2024-{:02}-01", m + 1);
            [
                json!({"t": t, "v": 10 + m, "s": "Self-serve customers in North America"}),
                json!({"t": t, "v": 30 - m, "s": "Enterprise customers in Western Europe"}),
            ]
        })
        .collect();
    for kind in ["line", "area"] {
        for width in [320.0, 390.0] {
            let g = at_width(
                json!({"type": kind, "title": "T", "encoding": {"x": "t", "y": "v", "color": "s"}}),
                &rows,
                width,
            );
            assert!(g.legend.is_some(), "{kind} {width}: long names go to a legend");
            let right = g.panels[0].plot.right();
            assert!(
                (right - (width - 9.0)).abs() <= 1e-6,
                "{kind} {width}: the plot ends at {right}; the endpoint halo needs 9px"
            );
        }
    }
}
