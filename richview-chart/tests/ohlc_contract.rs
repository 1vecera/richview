//! OHLC charts: a low-to-high line with a tick left at the open and right at the close, coloured
//! by direction, and the contract every type shares (FEA-CH-01, FEA-CH-57, FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::{Cmd, Geometry, Item};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"ohlc","title":"Daily prices","digits":4,"encoding":{"x":"day","open":"o","high":"h","low":"l","close":"c"},"axes":{"y":{"unit":"$","label":"Price"}}})
}

fn week() -> Vec<Value> {
    vec![
        json!({"day":"2026-03-02","o":100,"h":106,"l":98,"c":104}),
        json!({"day":"2026-03-03","o":104,"h":105,"l":97,"c":99}),
        json!({"day":"2026-03-04","o":99,"h":103,"l":99,"c":103}),
        json!({"day":"2026-03-05","o":103,"h":110,"l":101,"c":109.5}),
    ]
}

/// Every mark line as its end points.
fn lines(g: &Geometry) -> Vec<Vec<[f64; 2]>> {
    marks(g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Line { path, .. } => Some(
                path.cmds
                    .iter()
                    .filter_map(|c| match c {
                        Cmd::M { p } | Cmd::L { p } => Some(*p),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

fn dense() -> Vec<Value> {
    let months = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut price = 100.0f64;
    (0..250)
        .map(|i| {
            let (mut m, mut d) = (0, i);
            while d >= months[m] {
                d -= months[m];
                m += 1;
            }
            let open = price;
            let close = open + ((i * 7 % 11) as f64 - 5.0) * 0.4;
            price = close;
            json!({"day": format!("2026-{:02}-{:02}", m + 1, d + 1), "o": open, "h": open.max(close) + 1.0, "l": open.min(close) - 1.0, "c": close})
        })
        .collect()
}

#[test]
fn the_golden_week_meets_the_shared_contract() {
    contract(&spec(), &week());
}

#[test]
fn each_bar_is_a_range_line_with_ticks_at_the_open_and_the_close() {
    let g = draw(&spec(), &week());
    let l = lines(&g);
    assert_eq!(l.len(), 12, "three lines per bar and no bodies");
    assert!(marks(&g).iter().all(|i| matches!(i, Item::Line { .. })));
    for (k, item) in marks(&g).iter().enumerate() {
        let Item::Line { obs, .. } = item else { unreachable!() };
        assert_eq!(*obs, Some(k / 3), "the wick and both ticks belong to their period");
    }
    let per_unit = {
        let r = &l[0];
        (r[1][1] - r[0][1]).abs() / 8.0
    };
    for (k, (o, h, lo, c)) in [(100.0, 106.0, 98.0, 104.0), (104.0, 105.0, 97.0, 99.0)]
        .into_iter()
        .enumerate()
    {
        let (range, left, right) = (&l[3 * k], &l[3 * k + 1], &l[3 * k + 2]);
        let x = range[0][0];
        assert_eq!(range[0][0], range[1][0], "the range line is vertical");
        assert!(((range[1][1] - range[0][1]).abs() - (h - lo) * per_unit).abs() < 0.01);
        assert!(
            left[0][0] < x && (left[1][0] - x).abs() < 0.01,
            "the open tick leaves to the left"
        );
        assert!(
            right[0][0] > x - 0.01 && right[1][0] > x,
            "the close tick leaves to the right"
        );
        assert_eq!(left[0][1], left[1][1]);
        assert_eq!(right[0][1], right[1][1]);
        // Higher prices are higher up: the open tick sits (c - o) per_unit from the close tick.
        assert!(((left[0][1] - right[0][1]) - (c - o) * per_unit).abs() < 0.01);
    }
}

#[test]
fn readouts_give_the_four_prices_and_the_change() {
    let g = draw(&spec(), &week());
    let obs = observations(&g);
    assert_eq!(obs.len(), 4);
    assert_eq!(line(obs[0], "Open"), Some("$100"));
    assert_eq!(line(obs[0], "High"), Some("$106"));
    assert_eq!(line(obs[0], "Low"), Some("$98"));
    assert_eq!(line(obs[0], "Close"), Some("$104"));
    assert_eq!(line(obs[0], "Change"), Some("+$4"));
    assert_eq!(line(obs[1], "Change"), Some("−$5"));
    assert_eq!(line(obs[3], "Close"), Some("$109.5"));
    assert!(obs[1].readout.lines.iter().all(|l| l.derived == (l.name == "Change")));
    let a = obs[0].area.unwrap();
    assert!((a.w - g.panels[0].plot.w / 4.0).abs() < 0.01);
}

#[test]
fn up_and_down_are_two_coloured_series() {
    let g = draw(&spec(), &week());
    let names: Vec<&str> = g.series.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Up", "Down"]);
    assert!(g.legend.is_some());
    let series: Vec<Option<usize>> = marks(&g).iter().map(|i| i.series()).collect();
    assert_eq!(series[0], Some(0));
    assert_eq!(series[3], Some(1));
}

#[test]
fn a_high_or_low_that_contradicts_the_prices_fails_the_panel_and_names_the_row() {
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
    let mut rows = week();
    rows[2] = json!({"day":"2026-03-04","o":99,"h":103,"l":100,"c":103});
    let g = draw(&spec(), &rows);
    let c = check(&g, "source-error").expect("source-error");
    assert!(
        c.message.contains("Row 3") && c.message.contains("low"),
        "{}",
        c.message
    );
    assert!(g.failure.is_some() && !g.panels.is_empty());
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
    let g = draw(
        &spec(),
        &[
            json!({"day":"A","o":5,"h":5,"l":5,"c":5}),
            json!({"day":"B","o":5,"h":6,"l":4,"c":5}),
        ],
    );
    assert!(observations(&g)[0].area.unwrap().h >= 1.0, "a flat bar stays visible");
}

#[test]
fn dense_bars_keep_every_observation_and_thin_the_names() {
    let rows = dense();
    let g = draw_at(&spec(), &rows, 320.0);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 250);
    assert!(check(&g, "label-thinned").is_some(), "names thin out: {:?}", g.checks);
    contract(&spec(), &rows);
}

#[test]
fn horizontal_bars_run_down_the_page() {
    let mut s = spec();
    s["style"] = json!({"orientation":"horizontal"});
    let g = draw(&s, &week());
    let ys: Vec<f64> = observations(&g).iter().map(|o| o.area.unwrap().center()[1]).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
    contract(&s, &week());
}

#[test]
fn facets_scale_together_and_keep_their_own_bars() {
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
    gallery("ohlc", &spec(), &week());
    gallery("ohlc-dense", &spec(), &dense());
}
