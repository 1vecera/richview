//! Word clouds: sizes on a square-root scale, horizontal words packed without overlap inside the
//! plot, words that find no room reported, and the contract every type shares (FEA-CH-75).

mod common;

use common::*;
use richview_chart::Level;
use richview_chart::geometry::{Geometry, Item, Role, Text};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/wordcloud.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec() -> Value {
    json!({"type":"wordcloud","title":"T","encoding":{"label":"w","y":"v"}})
}

fn words(vs: &[(&str, f64)]) -> Vec<Value> {
    vs.iter().map(|(w, v)| json!({"w": w, "v": v})).collect()
}

fn placed(g: &Geometry) -> Vec<&Text> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label => Some(t),
            _ => None,
        })
        .collect()
}

fn unplaced(g: &Geometry) -> usize {
    g.dropped.iter().filter(|d| d.kind.as_deref() == Some("word")).count()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("wordcloud", &spec, &rows);
}

#[test]
fn words_are_horizontal_inside_the_plot_and_never_overlap() {
    let (spec, rows) = fixture();
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        let plot = g.panels[0].plot;
        let ws = placed(&g);
        assert!(!ws.is_empty());
        for (i, a) in ws.iter().enumerate() {
            assert_eq!(a.rotate, 0.0);
            assert!(a.bounds.inside(&plot, 0.0), "{width}: '{}' leaves the plot", a.text);
            for b in &ws[i + 1..] {
                assert!(
                    !a.bounds.overlaps(&b.bounds, 0.0),
                    "{width}: '{}' overlaps '{}'",
                    a.text,
                    b.text
                );
            }
        }
        assert_eq!(
            ws.len() + unplaced(&g),
            rows.len(),
            "{width}: every word is drawn or reported"
        );
    }
}

#[test]
fn size_follows_the_square_root_of_the_value() {
    let (spec, rows) = fixture();
    let g = draw_at(&spec, &rows, 1440.0);
    let grid: richview_chart::payload::Quant =
        serde_json::from_value(g.derived.iter().find(|d| d.key == "wordcloud.plan").unwrap().value["grid"].clone())
            .unwrap();
    let value_of = |word: &str| {
        rows.iter()
            .find(|r| r["topic"] == word)
            .and_then(|r| r["requests"].as_f64())
            .unwrap()
    };
    let (vmin, vmax) = (4.0f64, 48.0f64);
    let ws = placed(&g);
    let smallest = ws.iter().map(|t| t.size).fold(f64::INFINITY, f64::min);
    let largest = ws.iter().map(|t| t.size).fold(0.0, f64::max);
    assert!(largest > smallest * 2.0);
    let biggest = ws.iter().find(|t| t.text == "dashboards").unwrap();
    assert_eq!(biggest.size, largest);
    let lowest = ws.iter().find(|t| t.text == "drift").unwrap();
    assert_eq!(lowest.size, smallest);
    for t in &ws {
        let want = (value_of(&t.text).sqrt() - vmin.sqrt()) / (vmax.sqrt() - vmin.sqrt());
        let got = (t.size - smallest) / (largest - smallest);
        // Within a base step: a refined grid places a size shown coarser than its step no
        // finer than it shows (TYP-36).
        let step = (grid.max - grid.min) / f64::from(grid.steps.min(richview_chart::payload::QUANT_STEPS));
        assert!((want - got).abs() <= step, "{}: {got} vs {want}", t.text);
    }
}

#[test]
fn a_narrow_chart_reports_the_words_it_has_no_room_for() {
    let (spec, rows) = fixture();
    let wide = draw_at(&spec, &rows, 1440.0);
    let narrow = draw_at(&spec, &rows, 320.0);
    assert_eq!(unplaced(&wide), 0);
    assert!(placed(&narrow).len() <= placed(&wide).len());
    if unplaced(&narrow) > 0 {
        assert!(check(&narrow, "label-dropped").is_some());
        // A dropped word is still in the readouts.
        assert_eq!(observations(&narrow).len(), rows.len());
    }
}

#[test]
fn the_keyboard_reaches_every_word_once_whichever_words_find_room() {
    // Which words find room depends on the width and the style's type; the keyboard order does
    // not, so every style reaches the same rows, each once (cross-style, every-block chart 29).
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{ChartTheme, Options, layout};
    let rows = crowd(120);
    let block = ChartBlock::from_value(&spec()).unwrap();
    let mut left_out = std::collections::BTreeSet::new();
    for width in WIDTHS {
        for (size, font) in [(12.0, "Geist, system-ui, sans-serif"), (15.0, "Georgia, serif")] {
            let theme = ChartTheme {
                size,
                ui_font: font.into(),
                data_font: font.into(),
                ..ChartTheme::default()
            };
            let g = layout(
                &block,
                &rows,
                &theme,
                &Options {
                    width,
                    ..Options::default()
                },
            );
            left_out.insert(unplaced(&g));
            let mut reached: Vec<usize> = richview_chart::hit::order(&g)
                .iter()
                .map(|t| g.panels[t.panel].observations[t.observation].row.unwrap())
                .collect();
            reached.sort_unstable();
            assert_eq!(
                reached,
                (0..rows.len()).collect::<Vec<_>>(),
                "{width} px, {size} px type"
            );
        }
    }
    assert!(
        left_out.len() > 1,
        "the layouts leave out different numbers of words: {left_out:?}"
    );
}

#[test]
fn readouts_give_the_value_and_a_derived_share() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let o = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading == "dashboards")
        .unwrap();
    assert_eq!(line(o, "Requests"), Some("48"));
    let share = o.readout.lines.iter().find(|l| l.name == "Share").unwrap();
    assert!(share.derived && share.value.ends_with('%'));
}

#[test]
fn one_word_is_centred_and_equal_words_are_equal_sizes() {
    let one = words(&[("alone", 3.0)]);
    let g = draw(&spec(), &one);
    let plot = g.panels[0].plot;
    let t = placed(&g)[0];
    assert!((t.bounds.center()[0] - plot.center()[0]).abs() < 1.0);
    assert!((t.bounds.center()[1] - plot.center()[1]).abs() < 1.0);
    contract(&spec(), &one);
    let g = draw(&spec(), &words(&[("a", 2.0), ("b", 2.0), ("c", 2.0)]));
    let sizes: Vec<f64> = placed(&g).iter().map(|t| t.size).collect();
    assert!(sizes.iter().all(|s| (s - sizes[0]).abs() < 1e-9));
}

#[test]
fn a_word_wider_than_the_plot_is_reported_not_clipped() {
    let long = "x".repeat(80);
    let rows = words(&[(&long, 5.0), ("short", 3.0)]);
    let g = draw_at(&spec(), &rows, 320.0);
    assert!(g.failure.is_none());
    assert_eq!(unplaced(&g), 1);
    assert_eq!(placed(&g).len(), 1);
    assert!(check(&g, "clipping").is_none());
}

fn crowd(n: usize) -> Vec<Value> {
    (0..n)
        .map(|i| json!({"w": format!("w{i}"), "v": 1.0 + (i * 13 % 50) as f64}))
        .collect()
}

#[test]
fn a_panel_over_the_word_limit_is_not_drawn_and_names_the_limit() {
    let g = draw_at(&spec(), &crowd(151), 1440.0);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["count"], 151);
    assert_eq!(c.details["limit"], 150);
    assert_eq!(c.details["what"], "words per panel");
    assert!(observations(&g).is_empty());
}

#[test]
fn a_panel_at_the_word_limit_draws_and_stays_inside() {
    let rows = crowd(150);
    let g = draw_at(&spec(), &rows, 1440.0);
    assert!(g.failure.is_none());
    assert!(check(&g, "limit-exceeded").is_none());
    assert_eq!(observations(&g).len(), 150);
    assert!(nonfinite(&g).is_none());
    contract(&spec(), &rows[..80]);
}

#[test]
fn zero_negative_and_missing_values_are_left_out_and_bad_text_fails() {
    let mut rows = words(&[("up", 4.0), ("down", 2.0), ("none", 0.0), ("minus", -3.0)]);
    rows.push(json!({"w": "gap", "v": null}));
    rows.push(json!({"v": 5.0}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none());
    assert_eq!(placed(&g).len(), 2);
    assert!(check(&g, "excluded-observations").is_some());
    rows.push(json!({"w": "bad", "v": "lots"}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    stays_visible(&spec(), &[]);
    stays_visible(&spec(), &words(&[("zero", 0.0)]));
}

#[test]
fn a_missing_channel_fails_with_a_source_error() {
    let s = json!({"type":"wordcloud","title":"T","encoding":{"label":"w"}});
    let g = draw(&s, &words(&[("a", 1.0)]));
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn extreme_ranges_stay_finite() {
    let g = draw(&spec(), &words(&[("huge", 1e300), ("tiny", 1e-300), ("mid", 1.0)]));
    assert!(nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 3);
    let g = draw(&spec(), &words(&[("a", 1e-300), ("b", 3e-300)]));
    assert!(nonfinite(&g).is_none());
    assert_eq!(placed(&g).len(), 2);
}

#[test]
fn facets_share_one_size_scale() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("f");
    let mut rows = Vec::new();
    for (f, vs) in [("p", [9.0, 4.0, 1.0]), ("q", [4.0, 4.0, 2.0])] {
        for (i, v) in vs.iter().enumerate() {
            rows.push(json!({"w": format!("{f}{i}"), "v": v, "f": f}));
        }
    }
    let g = draw_at(&s, &rows, 1440.0);
    assert_eq!(g.panels.len(), 2);
    let size = |panel: usize, word: &str| {
        g.panels[panel]
            .items
            .iter()
            .find_map(|i| match i {
                Item::Text(t) if t.text == word => Some(t.size),
                _ => None,
            })
            .unwrap()
    };
    assert!((size(0, "p1") - size(1, "q0")).abs() < 1e-9);
    contract(&s, &rows);
}

#[test]
fn the_runtime_places_the_words_the_exact_rows_placed() {
    // 100.01 and 100.04 share a step of the grid, and 0.05 is zero on it.
    let rows = words(&[
        ("Gamma", 1000.0),
        ("Beta", 100.01),
        ("Alpha", 100.04),
        ("Delta", 5.0),
        ("Tiny", 0.05),
        ("Rest", 300.5),
    ]);
    runtime_matches_native(&spec(), &rows);
}
