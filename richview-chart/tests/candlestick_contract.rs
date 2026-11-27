//! Candlestick charts: wicks and bodies at their true prices, direction, validation and the
//! contract every type shares (FEA-CH-01, FEA-CH-57, FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::{Cmd, Item};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"candlestick","title":"Daily prices","digits":4,"encoding":{"x":"day","open":"o","high":"h","low":"l","close":"c"},"axes":{"y":{"unit":"$","label":"Price"}}})
}

fn week() -> Vec<Value> {
    vec![
        json!({"day":"2026-03-02","o":100,"h":106,"l":98,"c":104}),
        json!({"day":"2026-03-03","o":104,"h":105,"l":97,"c":99}),
        json!({"day":"2026-03-04","o":99,"h":103,"l":99,"c":103}),
        json!({"day":"2026-03-05","o":103,"h":110,"l":101,"c":109.5}),
    ]
}

/// The vertical extent of a wick line.
fn span(item: &Item) -> (f64, f64) {
    let Item::Line { path, .. } = item else {
        panic!("not a line")
    };
    let ys: Vec<f64> = path
        .cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::M { p } | Cmd::L { p } => Some(p[1]),
            _ => None,
        })
        .collect();
    (
        ys.iter().copied().fold(f64::INFINITY, f64::min),
        ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    )
}

#[test]
fn the_golden_week_meets_the_shared_contract() {
    contract(&spec(), &week());
}

#[test]
fn bodies_and_wicks_sit_at_their_true_prices() {
    let g = draw(&spec(), &week());
    let obs = observations(&g);
    assert_eq!(obs.len(), 4);
    let plot = g.panels[0].plot;
    // Price grows upwards: a higher high is a smaller y, and the scale is linear.
    let area = |i: usize| obs[i].area.unwrap();
    assert!(area(3).y < area(0).y);
    let per_unit = (area(0).h) / (106.0 - 98.0);
    assert!(per_unit > 0.0);
    assert!((area(1).h / (105.0 - 97.0) - per_unit).abs() < 0.01);
    assert!((area(3).h / (110.0 - 101.0) - per_unit).abs() < 0.01);
    // The first candle's body spans open 100 to close 104 inside its wick 98 to 106.
    let wicks: Vec<&Item> = marks(&g)
        .into_iter()
        .filter(|i| matches!(i, Item::Line { .. }))
        .collect();
    assert_eq!(wicks.len(), 4);
    for (k, wick) in wicks.iter().enumerate() {
        let Item::Line { obs, .. } = wick else { unreachable!() };
        assert_eq!(*obs, Some(k), "each wick belongs to the candle's observation");
    }
    let (top, bottom) = span(wicks[0]);
    assert!((bottom - top - 8.0 * per_unit).abs() < 0.01);
    let bodies: Vec<_> = marks(&g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(bodies.len(), 4);
    assert!((bodies[0].h - 4.0 * per_unit).abs() < 0.01);
    assert!((bodies[0].bottom() - (bottom - 2.0 * per_unit)).abs() < 0.01);
    assert!((bodies[0].y - (top + 2.0 * per_unit)).abs() < 0.01);
    // Every wick stays inside the plot, and the axis does not have to reach zero.
    assert!(top >= plot.y - 0.01 && bottom <= plot.bottom() + 0.01);
    assert!(
        g.panels[0]
            .items
            .iter()
            .all(|i| i.role() != richview_chart::geometry::Role::Baseline)
    );
}

#[test]
fn readouts_give_the_four_prices_and_the_change() {
    let g = draw(&spec(), &week());
    let obs = observations(&g);
    assert_eq!(obs[0].readout.heading, "Mar 2, 2026");
    assert_eq!(line(obs[0], "Open"), Some("$100"));
    assert_eq!(line(obs[0], "High"), Some("$106"));
    assert_eq!(line(obs[0], "Low"), Some("$98"));
    assert_eq!(line(obs[0], "Close"), Some("$104"));
    assert_eq!(line(obs[0], "Change"), Some("+$4"));
    assert_eq!(line(obs[1], "Change"), Some("−$5"));
    assert_eq!(line(obs[3], "Close"), Some("$109.5"));
    let derived: Vec<(&str, bool)> = obs[1]
        .readout
        .lines
        .iter()
        .map(|l| (l.name.as_str(), l.derived))
        .collect();
    assert_eq!(
        derived,
        [
            ("Open", false),
            ("High", false),
            ("Low", false),
            ("Close", false),
            ("Change", true)
        ]
    );
    // The hit area runs from the low to the high across the whole band.
    let a = obs[0].area.unwrap();
    assert!((a.w - g.panels[0].plot.w / 4.0).abs() < 0.01);
}

#[test]
fn up_and_down_are_two_series_and_up_is_hollow() {
    let g = draw(&spec(), &week());
    let names: Vec<&str> = g.series.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Up", "Down"]);
    assert!(g.legend.is_some());
    let bodies: Vec<_> = marks(&g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Rect {
                fill, stroke, series, ..
            } => Some((fill.color, stroke.is_some(), *series)),
            _ => None,
        })
        .collect();
    // A close equal to the open counts as Up (candle three closes at 103 from 99: up; none flat here).
    assert_eq!(
        bodies.iter().map(|b| b.2).collect::<Vec<_>>(),
        [Some(0), Some(1), Some(0), Some(0)]
    );
    assert!(bodies[0].1 && !bodies[1].1, "Up has an outline, Down is filled");
    assert_ne!(bodies[0].0, bodies[1].0, "a hollow body is the surface colour");
}

#[test]
fn a_flat_candle_is_up_and_stays_visible() {
    let g = draw(
        &spec(),
        &[
            json!({"day":"A","o":5,"h":5,"l":5,"c":5}),
            json!({"day":"B","o":5,"h":6,"l":4,"c":5}),
        ],
    );
    assert!(g.failure.is_none());
    let obs = observations(&g);
    assert_eq!(line(obs[0], "Change"), Some("$0"));
    assert!(obs[0].area.unwrap().h >= 1.0);
    let bodies: Vec<_> = marks(&g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Rect { rect, series, .. } => Some((rect.h, *series)),
            _ => None,
        })
        .collect();
    assert!(bodies.iter().all(|b| b.0 >= 1.0 && b.1 == Some(0)));
}

#[test]
fn decals_make_up_distinct_without_colour() {
    let s = spec();
    let g = draw_decals(&s, &week());
    let hatches: Vec<_> = marks(&g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Rect { fill, .. } => Some(fill.hatch),
            _ => None,
        })
        .collect();
    assert!(hatches.iter().all(Option::is_some), "{hatches:?}");
    assert_ne!(hatches[0], hatches[1]);
}

fn draw_decals(spec: &Value, rows: &[Value]) -> richview_chart::geometry::Geometry {
    let block = richview_chart::block::ChartBlock::from_value(spec).expect("a chart block");
    let theme: richview_chart::layout::ChartTheme = serde_json::from_value(json!({"decals": true})).expect("theme");
    richview_chart::layout::layout(&block, rows, &theme, &richview_chart::layout::Options::default())
}

#[test]
fn a_high_or_low_that_contradicts_the_body_fails_the_panel_and_names_the_row() {
    let mut rows = week();
    rows[1] = json!({"day":"2026-03-03","o":104,"h":103,"l":97,"c":99});
    let g = draw(&spec(), &rows);
    assert!(
        g.failure.as_deref().is_some_and(|f| f.contains("row 2")),
        "{:?}",
        g.failure
    );
    let c = check(&g, "source-error").expect("source-error");
    assert!(
        c.message.contains("Row 2") && c.message.contains("high"),
        "{}",
        c.message
    );
    stays_failed(&g);
    let mut rows = week();
    rows[2] = json!({"day":"2026-03-04","o":99,"h":103,"l":100,"c":103});
    let g = draw(&spec(), &rows);
    let c = check(&g, "source-error").expect("source-error");
    assert!(
        c.message.contains("Row 3") && c.message.contains("low"),
        "{}",
        c.message
    );
    stays_failed(&g);
}

fn stays_failed(g: &richview_chart::geometry::Geometry) {
    assert!(g.failure.is_some());
    assert!(!g.panels.is_empty());
    assert!(nonfinite(g).is_none());
}

#[test]
fn missing_cells_are_disclosed_and_bad_text_fails_the_panel() {
    let mut rows = week();
    rows[2] = json!({"day":"2026-03-04","o":99,"h":103,"l":null,"c":103});
    let g = draw(&spec(), &rows);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(observations(&g).len(), 3);
    rows[2] = json!({"day":"2026-03-04","o":99,"h":"1,000","l":98,"c":103});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
}

#[test]
fn a_chart_without_its_four_channels_says_which() {
    let mut s = spec();
    s["encoding"] = json!({"x":"day","open":"o"});
    let g = draw(&s, &week());
    assert!(g.failure.as_deref().is_some_and(|f| f.contains("high, low, close")));
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&spec(), &[]);
    contract(&spec(), &[json!({"day":"Only","o":10,"h":12,"l":9,"c":11})]);
    contract(&spec(), &[json!({"day":"Flat","o":10,"h":10,"l":10,"c":10})]);
    contract(
        &spec(),
        &[
            json!({"day":"A","o":-5,"h":-1,"l":-9,"c":-7}),
            json!({"day":"B","o":-7,"h":2,"l":-8,"c":1}),
        ],
    );
    let g = draw(
        &spec(),
        &[
            json!({"day":"A","o":1e300,"h":2e300,"l":-1e300,"c":5e299}),
            json!({"day":"B","o":1,"h":2,"l":0,"c":1}),
        ],
    );
    assert!(nonfinite(&g).is_none());
}

fn dense() -> Vec<Value> {
    let mut price = 100.0f64;
    (0..250)
        .map(|i| {
            let open = price;
            let close = open + ((i * 7 % 11) as f64 - 5.0) * 0.4;
            price = close;
            let day = chrono_day(i);
            json!({"day": day, "o": open, "h": open.max(close) + 1.0, "l": open.min(close) - 1.0, "c": close})
        })
        .collect()
}

fn chrono_day(i: usize) -> String {
    let months = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let (mut m, mut d) = (0, i);
    while d >= months[m] {
        d -= months[m];
        m += 1;
    }
    format!("2026-{:02}-{:02}", m + 1, d + 1)
}

#[test]
fn dense_candles_thin_to_one_pixel_and_names_thin_out() {
    let rows = dense();
    let g = draw_at(&spec(), &rows, 320.0);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 250);
    for item in marks(&g) {
        if let Item::Rect { rect, .. } = item {
            assert!(rect.w >= 1.0 && rect.h >= 1.0, "{rect:?}");
        }
    }
    assert!(check(&g, "label-thinned").is_some(), "names thin out: {:?}", g.checks);
    contract(&spec(), &rows);
}

#[test]
fn horizontal_candles_run_prices_left_to_right() {
    let mut s = spec();
    s["style"] = json!({"orientation":"horizontal"});
    let g = draw(&s, &week());
    let obs = observations(&g);
    let ys: Vec<f64> = obs.iter().map(|o| o.area.unwrap().center()[1]).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
    assert!(obs[3].area.unwrap().right() > obs[0].area.unwrap().right());
    contract(&s, &week());
}

#[test]
fn facets_scale_together_and_keep_their_own_candles() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("stock");
    let rows = vec![
        json!({"day":"A","o":10,"h":12,"l":9,"c":11,"stock":"North"}),
        json!({"day":"B","o":11,"h":13,"l":10,"c":12,"stock":"North"}),
        json!({"day":"A","o":50,"h":52,"l":49,"c":51,"stock":"South"}),
        json!({"day":"B","o":51,"h":53,"l":50,"c":50,"stock":"South"}),
    ];
    let g = draw(&s, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(line(&g.panels[0].observations[1], "Close"), Some("$12"));
    assert_eq!(line(&g.panels[1].observations[1], "Change"), Some("−$1"));
    contract(&s, &rows);
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("candlestick", &spec(), &week());
    gallery("candlestick-dense", &spec(), &dense());
    let g = draw_decals(&spec(), &week());
    if let Ok(dir) = std::env::var("RICHVIEW_GALLERY_OUT") {
        let svg = richview_chart::vector::svg(&g, &richview_chart::layout::ChartTheme::default());
        std::fs::write(format!("{dir}/candlestick-decals.svg"), svg).expect("write");
    }
}

#[test]
fn readout_formats_date_like_the_axis() {
    let g = draw(&spec(), &week());
    let obs = observations(&g);
    assert_eq!(obs[0].readout.heading, "Mar 2, 2026");
}
