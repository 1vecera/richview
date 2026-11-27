//! Chart figures in rendered reports: the title and subtitle once in HTML, a box reserved from
//! the geometry model before the runtime mounts (FEA-CH-64), one encoding of series identity at
//! every width, and calendar axes that keep their years.
use super::common;

use common::{geometry, texts};
use richview_chart::geometry::{Anchor, Item, Role};
use richview_core::document::{RenderOptions, RenderedReport, ReportFolder, render};
use serde_json::{Value, json};

fn report_with(charts: &[Value], styles: Vec<String>) -> RenderedReport {
    let source = charts
        .iter()
        .map(|chart| format!("```chart\n{chart}\n```\n"))
        .collect::<Vec<_>>()
        .join("\n");
    let options = RenderOptions {
        styles,
        report_folder: Some(ReportFolder {
            root: "figures".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    render(&source, &options).expect("report renders")
}

fn report(charts: &[Value]) -> RenderedReport {
    report_with(charts, vec![])
}

/// Each `<figure>` of the report, from its opening tag to its closing tag.
fn figures(html: &str) -> Vec<&str> {
    html.match_indices("<figure")
        .map(|(at, _)| {
            let rest = &html[at..];
            &rest[..rest.find("</figure>").unwrap() + "</figure>".len()]
        })
        .collect()
}

fn rows(groups: &[&str], x: &[&str]) -> Value {
    let mut out = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        for (i, x) in x.iter().enumerate() {
            out.push(json!({"g": group, "x": x, "y": (g * 10 + i * 3 + 5) as f64}));
        }
    }
    Value::Array(out)
}

#[test]
fn the_caption_names_each_chart_once_and_the_chart_draws_no_heading() {
    let report = report(&[
        json!({"type":"bar","title":"Bars <rose> & \"fell\"","subtitle":"Quarterly, % of total","encoding":{"x":"x","y":"y"},"data":{"inline":rows(&["one"], &["A", "B"])}}),
        json!({"type":"line","title":"Interest rose","subtitle":"Weekly, 0–100","encoding":{"x":"x","y":"y","color":"g"},"data":{"inline":rows(&["Alpha", "Beta"], &["2026-01-01", "2026-02-01", "2026-03-01"])}}),
    ]);
    let html = &report.html;
    let figures = figures(html);
    assert_eq!(figures.len(), 2);
    let caption = figures[0]
        .split_once("<figcaption")
        .unwrap()
        .1
        .split_once("</figcaption>")
        .unwrap()
        .0;
    assert!(caption.contains(">Bars &lt;rose&gt; &amp; &quot;fell&quot;</p>"));
    assert!(caption.contains(">Quarterly, % of total</p>"));
    assert!(caption.find("rv-chart__title") < caption.find("rv-chart__subtitle"));
    for figure in &figures {
        assert_eq!(
            figure.matches("<figcaption").count(),
            1,
            "one title per figure"
        );
        assert_eq!(figure.matches("class=\"rv-chart__title\"").count(), 1);
    }
    for payload in &report.charts {
        for width in [358.0, 832.0] {
            let g = geometry(payload, width);
            let drawn = texts(&g, None);
            for heading in [
                "Bars <rose> & \"fell\"",
                "Quarterly, % of total",
                "Interest rose",
                "Weekly, 0–100",
            ] {
                assert!(
                    !drawn.contains(&heading),
                    "{width}: the chart draws no heading"
                );
            }
        }
    }
    // The base title rule; the phone rule's `.rv-chart__titlerow>.rv-chart__title{` is another.
    assert_eq!(
        html.matches(".rv-chart .rv-chart__title{").count(),
        1,
        "figure rules ship once"
    );
    assert!(
        html.rfind(".rv-chart__title{") > html.find(".rv-report__body"),
        "figure rules follow the theme rules"
    );
}

/// The reserved Reset action of a figure: its label and the kind of place its attribute keeps.
fn reserved_reset(figure: &str) -> Option<(String, String)> {
    let at = figure.find("rv-chart__action--reset")?;
    let button = &figure[figure[..at].rfind("<button")?..];
    let button = &button[..button.find("</button>")?];
    let label = button
        .split("<span>")
        .nth(1)?
        .split("</span>")
        .next()?
        .to_owned();
    let place = button
        .split("data-rv-place=\"")
        .nth(1)?
        .split('"')
        .next()?
        .to_owned();
    Some((label, place))
}

#[test]
fn a_chart_a_reader_turns_keeps_reset_views_place_as_a_zooming_chart_keeps_reset_zooms() {
    let solid = |kind: &str| {
        let points: Vec<Value> = (0..5)
            .flat_map(|i| {
                (0..5).map(move |j| {
                    let group = ["x", "y"][j % 2];
                    json!({"a": i, "b": j, "c": i * 3 + j * 2 + (i * j) % 3, "g": group})
                })
            })
            .collect();
        match kind {
            "bar3d" => {
                json!({"type": kind, "title": "T", "encoding": {"x": "a", "y": "c", "color": "g"}, "data": {"inline": points}})
            }
            _ => {
                json!({"type": kind, "title": "T", "encoding": {"x": "a", "y": "b", "z": "c"}, "data": {"inline": points}})
            }
        }
    };
    let report = report(&[
        solid("surface"),
        solid("scatter3d"),
        solid("bar3d"),
        json!({"type":"line","title":"T","encoding":{"x":"x","y":"y"},"data":{"inline":rows(&["A"], &["2026-01-01", "2026-02-01", "2026-03-01"])}}),
        json!({"type":"bar","title":"T","encoding":{"x":"x","y":"y"},"data":{"inline":rows(&["A"], &["A", "B"])}}),
    ]);
    let all = figures(&report.html);
    assert_eq!(all.len(), 5);
    for solid in &all[..3] {
        assert_eq!(
            reserved_reset(solid),
            Some(("Reset view".to_owned(), "view".to_owned())),
            "{solid}"
        );
    }
    assert_eq!(
        reserved_reset(all[3]),
        Some(("Reset zoom".to_owned(), "zoom".to_owned()))
    );
    assert_eq!(
        reserved_reset(all[4]),
        None,
        "a chart that neither zooms nor turns reserves none"
    );
}

#[test]
fn before_the_runtime_a_figure_reserves_its_box_and_shows_the_notice() {
    let chart = json!({"type":"bar","title":"Bars","encoding":{"x":"x","y":"y"},"data":{"inline":rows(&["one"], &["A", "B"])}});
    let ratio = |figure: &str, name: &str| -> f64 {
        let at = figure.find(&format!("{name}:")).unwrap() + name.len() + 1;
        let rest = &figure[at..];
        rest[..rest.find([';', '"']).unwrap()].parse().unwrap()
    };
    // Screen geometry uses the reading measure less the frame's inset (STY-30).
    for (styles, desktop, inset) in [
        (vec![], 672.0, 40.0),
        (vec!["air".into()], 672.0, 0.0),
        (vec!["brutalist".into()], 672.0, 40.0),
        (vec!["hand-drawn".into()], 672.0, 40.0),
        (
            vec!["sleek".into(), "tokens: {measure: 40rem}".into()],
            640.0,
            40.0,
        ),
        (vec!["scientific".into()], 672.0, 0.0),
    ] {
        let report = report_with(std::slice::from_ref(&chart), styles);
        let figure = figures(&report.html)[0];
        assert!(
            figure.contains("needs JavaScript and WebGL"),
            "ARC-05 notice"
        );
        // The action group is reserved with its final buttons, invisible and inert until the
        // runtime wires them (FEA-CH-43, FEA-CH-64).
        let group = figure
            .split_once("<div class=\"rv-chart__actions\"")
            .unwrap()
            .1;
        let (open, rest) = group.split_once('>').unwrap();
        let group = &rest[..rest.find("</div>").unwrap()];
        assert!(
            open.contains("data-rv-pending") && open.contains("inert"),
            "{open}"
        );
        for label in ["Table", "Download PNG", "CSV", "Expand"] {
            assert!(group.contains(&format!("<span>{label}</span>")), "{label}");
        }
        let outside = figure.replace(group, "");
        assert!(!outside.contains("<svg"), "no static frame (FEA-CH-64)");
        let payload = &report.charts[0];
        for (name, width) in [
            ("--rv-ratio-phone", 358.0 - inset),
            ("--rv-ratio", desktop - inset),
        ] {
            let g = geometry(payload, width);
            let expected = g.width / g.height;
            assert!(
                // The figure is laid out at the column less its frame's inset, where the bottom
                // clearance of the text may differ by a few pixels from this width's.
                (ratio(figure, name) - expected).abs() < 0.05,
                "{name}: the box follows the geometry at {width} px"
            );
        }
    }
}

#[test]
fn a_phone_keeps_two_icons_beside_the_title_wherever_the_style_sets_it() {
    let chart = json!({"type":"bar","title":"Bars","encoding":{"x":"x","y":"y"},"data":{"inline":rows(&["one"], &["A", "B"])}});
    // Up to 600 px the row never wraps and the two downloads leave the figure's own row; the
    // expanded view's group is not in a title row, so it keeps them (Daniel, 17 Nov 2026).
    let phone = "@media (max-width:600px){.rv-chart__titlerow{flex-wrap:nowrap}.rv-chart__titlerow>.rv-chart__title{flex-basis:0}.rv-chart__titlerow>.rv-chart__actions>.rv-chart__action--png,.rv-chart__titlerow>.rv-chart__actions>.rv-chart__action--csv{display:none}}";
    // A style with the title under the chart sets the icons over the chart on a wide screen; on
    // a phone its row is whole again, so they take no line of their own there either.
    let below = "@media (max-width:600px){.rv-chart .rv-chart__titlerow{display:flex}";
    for (style, title_below) in [
        ("sleek", false),
        ("air", false),
        ("scientific", false),
        ("brutalist", false),
        ("hand-drawn", true),
    ] {
        let html = report_with(std::slice::from_ref(&chart), vec![style.into()]).html;
        assert_eq!(html.matches(phone).count(), 1, "{style}");
        assert_eq!(html.contains(below), title_below, "{style}");
        // The base rule comes first, so the phone's rule of the same weight wins.
        assert!(
            html.find(".rv-chart .rv-chart__title{") < html.find(phone),
            "{style}"
        );
    }
}

#[test]
fn series_identity_is_drawn_once_at_every_width() {
    let report = report(&[
        json!({"type":"bar","title":"Grouped","encoding":{"x":"x","y":"y","color":"g"},"data":{"inline":rows(&["North", "South", "East"], &["Q1", "Q2"])}}),
        json!({"type":"line","title":"Shares","encoding":{"x":"x","y":"y","color":"g"},"data":{"inline":rows(&["Hacker News comments", "Wikipedia article views", "npm SDK downloads"], &["2026-01-01", "2026-02-01", "2026-03-01", "2026-04-01"])}}),
        json!({"type":"bar","title":"Single","encoding":{"x":"x","y":"y"},"data":{"inline":rows(&["one"], &["A", "B"])}}),
        json!({"type":"pie","title":"Slices","encoding":{"x":"x","y":"y","color":"x"},"data":{"inline":rows(&["one"], &["A", "B", "C"])}}),
    ]);
    for width in [320.0, 358.0, 480.0, 720.0, 832.0, 1100.0] {
        let grouped = geometry(&report.charts[0], width);
        let legend = grouped.legend.as_ref().expect("grouped bars need a legend");
        assert_eq!(legend.entries.len(), 3, "{width}");
        let lines = geometry(&report.charts[1], width);
        let labels = texts(&lines, Some(Role::Label));
        let named = lines
            .series
            .iter()
            .filter(|s| labels.contains(&s.name.as_str()))
            .count();
        assert!(
            lines.legend.is_some() != (named > 0),
            "{width}: legend or direct names, never both"
        );
        if named > 0 {
            assert_eq!(named, 3, "{width}: every series keeps its name");
            for s in &lines.series {
                assert_eq!(
                    labels.iter().filter(|l| **l == s.name).count(),
                    1,
                    "{width}"
                );
            }
        }
        for single in &report.charts[2..] {
            assert!(
                geometry(single, width).legend.is_none(),
                "{width}: one series or named slices need no legend"
            );
        }
    }
}

#[test]
fn calendar_axes_carry_four_digit_years_where_they_change() {
    let months: Vec<String> = (0..30)
        .map(|i| format!("{}-{:02}-01", 2024 + i / 12, i % 12 + 1))
        .collect();
    let months: Vec<&str> = months.iter().map(String::as_str).collect();
    let chart = json!({"type":"line","title":"Two years","encoding":{"x":"x","y":"y","color":"g"},"data":{"inline":rows(&["Alpha"],&months)}});
    let report = report(std::slice::from_ref(&chart));
    for width in [358.0, 832.0] {
        let g = geometry(&report.charts[0], width);
        let ticks: Vec<&str> = g
            .items()
            .filter_map(|item| match item {
                // Time ticks sit centred under their date; value ticks end at the axis.
                Item::Text(t) if t.role == Role::AxisLabel && t.anchor == Anchor::Middle => {
                    Some(t.text.as_str())
                }
                _ => None,
            })
            .collect();
        assert!(ticks.len() >= 2, "{width}: {ticks:?}");
        // FEA-CH-25: the first label and each label where the year changes carry the
        // four-digit year; the others show the period alone; no two-digit years.
        let year = |label: &str| {
            label
                .split_whitespace()
                .find(|w| w.len() == 4 && w.chars().all(|c| c.is_ascii_digit()))
                .map(str::to_owned)
        };
        assert!(
            year(ticks[0]).is_some(),
            "{width}: {} carries its year",
            ticks[0]
        );
        let mut last = year(ticks[0]);
        for label in &ticks[1..] {
            assert!(
                !label
                    .split_whitespace()
                    .any(|w| w.len() == 2 && w.chars().all(|c| c.is_ascii_digit())),
                "{width}: {label} has no two-digit year"
            );
            if let Some(y) = year(label) {
                assert_ne!(Some(&y), last.as_ref(), "{width}: {label} repeats its year");
                last = Some(y);
            }
        }
        assert!(
            ticks.iter().filter(|l| year(l).is_some()).count() >= 2,
            "{width}: {ticks:?}"
        );
    }
}

/// Each reference width reserves the legend its own layout draws, so the runtime's swap moves
/// nothing at either width (FEA-CH-33, FEA-CH-64).
#[test]
fn each_reference_width_reserves_its_own_legend() {
    // Two lines that lie on top of each other cannot be named inside the plot: their names stand
    // at the line ends at the desktop width and take a legend on the phone, where that column
    // would cost more than a quarter of the plot.
    let dates = [
        "2026-01-01",
        "2026-02-01",
        "2026-03-01",
        "2026-04-01",
        "2026-05-01",
        "2026-06-01",
    ];
    let mut shares = rows(&["OpenAI"], &dates).as_array().unwrap().clone();
    shares.extend(rows(&["Anthropic"], &dates).as_array().unwrap().clone());
    let lines = json!({"type":"line","title":"Shares","encoding":{"x":"x","y":"y","color":"g"},"data":{"inline":shares}});
    let bars = json!({"type":"bar","title":"Grouped","encoding":{"x":"x","y":"y","color":"g"},"data":{"inline":rows(&["North", "South"], &["Q1", "Q2"])}});
    let report = report(&[lines, bars]);
    let mut differed = false;
    for (payload, figure) in report.charts.iter().zip(figures(&report.html)) {
        let [phone, desktop] = [358.0, 512.0].map(|w| geometry(payload, w).legend.is_some());
        let classes = |name: &str| figure.matches(&format!("rv-chart__legend--{name}")).count();
        let statics = figure.matches("data-rv-static").count();
        if phone == desktop {
            assert_eq!((classes("phone"), classes("desktop")), (0, 0));
            assert_eq!(statics, usize::from(phone), "one shared legend or none");
        } else {
            differed = true;
            assert_eq!(classes("phone"), usize::from(phone));
            assert_eq!(classes("desktop"), usize::from(desktop));
        }
    }
    assert!(differed, "the two shares take a legend on the phone only");
}

/// A chart with reader controls embeds every row and field its states need, reserves the control
/// row with the initial state, and names any state that fails its checks (FEA-BR-01 to
/// FEA-BR-06, FEA-BR-32, FEA-BR-33).
#[test]
fn controls_reserve_their_row_and_every_state_is_checked() {
    let mut data = Vec::new();
    for (y, year) in ["1960", "2000"].iter().enumerate() {
        for (r, region) in ["Europe", "Asia"].iter().enumerate() {
            data.push(json!({"year": year, "region": region, "income": 1000 * (y + r + 1), "life": 50 + 10 * y + r, "life_index": 1.23456 * (y + 1) as f64, "unused": 1}));
        }
    }
    // A metric whose second view cannot draw: a value text that is not a number.
    data.push(json!({"year": "2000", "region": "Asia", "income": 900, "life": 60, "life_index": "lots", "unused": 2}));
    let chart = json!({"type": "scatter", "title": "Lives", "encoding": {"x": "income", "y": "life", "color": "region"},
    "data": {"inline": data},
    "controls": [
        {"kind": "playback", "id": "year", "field": "year", "label": "Year", "initial": "1960"},
        {"kind": "filter", "id": "region", "field": "region", "label": "Region", "options": ["Europe", "Asia"], "all": "World"},
        {"kind": "metric", "id": "m", "label": "Measure", "options": [
            {"value": "life", "label": "Years", "field": "life", "axis": {"label": "Life expectancy"}},
            {"value": "index", "label": "Index", "field": "life_index", "axis": {"label": "Indexed"}}
        ]}
    ]});
    let report = report(std::slice::from_ref(&chart));
    let payload = &report.charts[0];
    let columns: Vec<&str> = payload["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    for field in ["year", "region", "income", "life", "life_index"] {
        assert!(
            columns.contains(&field),
            "{field} travels for its state: {columns:?}"
        );
    }
    assert!(!columns.contains(&"unused"), "no field no state reads");
    assert_eq!(
        payload["rows"].as_array().unwrap().len(),
        5,
        "every row, every frame"
    );
    let figure = figures(&report.html)[0];
    let row = figure
        .split_once("<div class=\"rv-chart__controls\"")
        .unwrap()
        .1;
    assert!(
        row.starts_with(" role=\"group\" aria-label=\"Chart controls\" data-rv-pending inert>")
    );
    assert!(row.contains("aria-valuetext=\"1960\""), "the initial frame");
    assert!(
        row.contains("aria-pressed=\"true\">World</button>"),
        "the first selector as buttons"
    );
    assert!(
        row.contains("<select class=\"rv-chart__select\">"),
        "the second as a dropdown"
    );
    assert!(
        figure.contains(
            "<p class=\"rv-chart__state\">Year: 1960 · Region: World · Measure: Years</p>"
        )
    );
    let state = report
        .diagnostics
        .iter()
        .find(|d| d.message.contains("(state "))
        .unwrap_or_else(|| {
            panic!(
                "the failing state is named: {:?}",
                report
                    .diagnostics
                    .iter()
                    .map(|d| &d.message)
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        state.message.contains("Measure: Index"),
        "{}",
        state.message
    );
}

#[test]
fn quoted_numbers_on_the_x_of_a_density_or_ecdf_are_rounded_like_other_numbers() {
    let data: Vec<Value> = [
        "10.123456789012",
        "20.987654321098",
        "30.555555555555",
        "41.111111111111",
    ]
    .iter()
    .map(|x| json!({"x": x}))
    .collect();
    for kind in ["density", "ecdf"] {
        let report = report(&[
            json!({"type": kind, "title": "Values", "encoding": {"x": "x"}, "data": {"inline": data}}),
        ]);
        let payload = &report.charts[0];
        let at = payload["columns"]
            .as_array()
            .unwrap()
            .iter()
            .position(|c| c == "x")
            .expect("x travels");
        for row in payload["rows"].as_array().unwrap() {
            assert!(
                row[at].is_number(),
                "{kind}: {} is rounded to a number",
                row[at]
            );
        }
    }
}

#[test]
fn positions_travel_on_a_grid_and_readouts_come_from_the_exact_rows() {
    let ys = [100.016, 100.4, 100.88, 101.21, 101.69];
    let data: Vec<Value> = ys
        .iter()
        .enumerate()
        .map(|(i, y)| json!({"k": format!("c{i}"), "y": y}))
        .collect();
    let out = report(&[json!({
        "type": "bar", "title": "Values",
        "encoding": {"x": "k", "y": "y"}, "data": {"inline": data},
    })]);
    let payload = &out.charts[0];
    let grid = &payload["quant"]["y"];
    let (min, max) = (grid["min"].as_f64().unwrap(), grid["max"].as_f64().unwrap());
    assert!(
        grid["steps"]
            .as_u64()
            .is_some_and(|n| (4000..=4096).contains(&n)),
        "{grid}"
    );
    // The grid's ends sit one digit finer than the span's leading digit, not at the source's.
    assert_eq!((min, max), (100.0, 101.7));
    let at = payload["columns"]
        .as_array()
        .unwrap()
        .iter()
        .position(|c| c == "y")
        .unwrap();
    // Rows hold whole grid steps, and a step maps back inside the grid.
    for row in payload["rows"].as_array().unwrap() {
        let k = row[at].as_u64().expect("a grid step");
        assert!(k <= 4096, "{k}");
    }
    // No source value is written as such: every number token is a grid step or a readout.
    for source in ["100.016", "101.69", "100.88", "101.21"] {
        for edge in [",", "["] {
            for end in [",", "]"] {
                let token = format!("{edge}{source}{end}");
                assert!(!out.html.contains(&token), "{source} reached the HTML");
            }
        }
    }
    // Table and CSV read the author's precision.
    assert_eq!(payload["shown"]["y"][0], json!(100));
    assert_eq!(payload["shown"]["y"][4], json!(102));
    // Readouts were laid out from the exact rows and are text.
    assert!(!payload["text"].as_array().unwrap().is_empty());
}

#[test]
fn extra_numeric_channels_travel_on_their_own_grid() {
    let data: Vec<Value> = [
        (1, 100.016, 101.69, 99.88, 101.21),
        (2, 101.21, 102.4, 100.5, 102.0),
    ]
    .iter()
    .map(|(d, o, h, l, c)| json!({"d": format!("2024-01-0{d}"), "o": o, "h": h, "l": l, "c": c}))
    .collect();
    let out = report(&[json!({
        "type": "ohlc", "title": "Prices",
        "encoding": {"x": "d", "open": "o", "high": "h", "low": "l", "close": "c"},
        "data": {"inline": data},
    })]);
    let quant = out.charts[0]["quant"].as_object().unwrap();
    for column in ["o", "h", "l", "c"] {
        assert!(quant.contains_key(column), "{column}: {quant:?}");
    }
}

#[test]
fn small_integer_scales_use_unit_steps_without_treating_decimal_interiors_as_integers() {
    for ys in [vec![-3.0, 0.0, 8.0], vec![10001.0, 10002.0, 10004.0]] {
        let data: Vec<Value> = ys
            .iter()
            .enumerate()
            .map(|(i, y)| json!({"x": format!("c{i}"), "y": y}))
            .collect();
        let out = report(&[json!({
            "type": "bar", "title": "Integers",
            "encoding": {"x": "x", "y": "y"}, "data": {"inline": data},
        })]);
        let payload = common::payload(&out.charts[0]);
        let grid = &payload.quant["y"];
        assert_eq!(grid.resolution, Some(1.0));
        assert_eq!((grid.max - grid.min) / f64::from(grid.steps), 1.0);
        for value in ys {
            assert_eq!(grid.snap(value), value, "{grid:?}");
        }
    }
    let out = report(&[json!({
        "type": "bar", "title": "Decimals",
        "encoding": {"x": "x", "y": "y"},
        "data": {"inline": [{"x":"a", "y":1}, {"x":"b", "y":1.25}, {"x":"c", "y":2}]},
    })]);
    let payload = common::payload(&out.charts[0]);
    let grid = &payload.quant["y"];
    assert_eq!(grid.resolution, None);
    assert!(
        grid.steps > 1000,
        "decimal interiors need their grid: {grid:?}"
    );
    assert!((grid.snap(1.25) - 1.25).abs() < 0.001);
}

#[test]
fn large_values_in_the_payload_keep_only_the_authors_shown_digits() {
    for extra_zero in [false, true] {
        let mut data = vec![
            json!({"x":"a", "y":1234567.891}),
            json!({"x":"b", "y":167543.217}),
        ];
        if extra_zero {
            data.push(json!({"x":"zero", "y":0}));
        }
        let out = report(&[json!({
            "type": "bar", "title": "Large values",
            "encoding": {"x": "x", "y": "y"},
            "data": {"inline": data},
        })]);
        assert_eq!(out.charts[0]["shown"]["y"][0], json!(1230000));
        assert_eq!(out.charts[0]["shown"]["y"][1], json!(168000));
        assert!(
            out.charts[0]["quant"]["y"]["steps"].as_u64().unwrap() <= 4095,
            "a whole shown value must not imply unit precision: {}",
            out.charts[0]["quant"]
        );
        for forbidden in ["1234567", "1234568", "167543"] {
            assert!(
                !out.html.contains(forbidden),
                "{forbidden} reached the HTML"
            );
        }
    }
}

#[test]
fn embedded_statistics_and_bin_edges_keep_the_authors_precision() {
    let values = [1.23456789, 2.3456789, 3.456789, 4.5678912, 9.87654321];
    let data: Vec<Value> = values.iter().map(|v| json!({"g":"a", "v":v})).collect();
    for (kind, encoding, key) in [
        ("boxplot", json!({"x":"g", "y":"v"}), "boxplot.summaries"),
        ("histogram", json!({"x":"v"}), "histogram.edges"),
        ("dotplot", json!({"x":"v"}), "dotplot.edges"),
    ] {
        let out = report(&[json!({
            "type":kind, "title":kind, "encoding":encoding, "data":{"inline":data},
        })]);
        let payload = common::payload(&out.charts[0]);
        let record = payload
            .derived
            .iter()
            .flat_map(|s| &s.derived)
            .find(|d| d.key == key)
            .expect("the embedded derived record");
        if kind == "boxplot" {
            let grid: richview_chart::payload::Quant =
                serde_json::from_value(record.value["grid"].clone())
                    .expect("box statistics carry their position grid");
            let raw = &payload.quant["v"];
            assert_eq!((grid.min, grid.max), (raw.min, raw.max));
            assert!(grid.steps >= raw.steps && grid.steps >= 4095);
            if raw.steps >= 4095 {
                assert_eq!(&grid, raw, "a finer supplied grid stays intact");
            } else {
                assert_eq!(grid.resolution, None);
            }
            let summary = &record.value["groups"][0];
            let expected = [
                ("min", values[0]),
                ("q1", values[1]),
                ("median", values[2]),
                ("q3", values[3]),
                ("max", values[4]),
                ("lower_whisker", values[0]),
                ("upper_whisker", values[3]),
            ];
            for (field, source) in expected {
                let coordinate = &summary[field];
                assert_eq!(coordinate.as_object().unwrap().len(), 1);
                let index = coordinate["grid"]
                    .as_u64()
                    .expect("statistics travel as integer grid indices");
                assert!(index <= u64::from(grid.steps), "{field}: {index}");
                let decoded = grid.at(index);
                let half_step = (grid.max - grid.min) / f64::from(grid.steps) / 2.0;
                assert!(
                    (decoded - source).abs() <= half_step + 1e-12,
                    "{field}: {decoded} is not the source position {source} on {grid:?}"
                );
            }
            // Readout text stays separate from position indices, at the source's shown precision.
            for (field, source) in expected
                .into_iter()
                .filter(|(field, _)| !matches!(*field, "min" | "max"))
            {
                let shown = richview_chart::numbers::value(
                    source,
                    &richview_chart::numbers::Format::default(),
                );
                assert_eq!(summary["text"][field], shown);
            }
            continue;
        }
        fn assert_precision(value: &Value) {
            match value {
                Value::Number(n) if n.is_f64() => {
                    let v = n.as_f64().unwrap();
                    let shown = richview_chart::numbers::round_sig(v, 3);
                    assert!(
                        (v - shown).abs() <= v.abs().max(1.0) * 1e-12,
                        "derived {v} retains digits beyond {shown}"
                    );
                }
                Value::Array(values) => values.iter().for_each(assert_precision),
                Value::Object(values) => values.values().for_each(assert_precision),
                _ => {}
            }
        }
        assert_precision(&record.value);
    }
}

#[test]
fn integer_box_observations_keep_native_resolution_and_fractional_quartiles_keep_their_geometry() {
    let out = report(&[json!({
        "type":"boxplot", "title":"Integer observations", "encoding":{"x":"g", "y":"v"},
        "data":{"inline":[{"g":"a","v":1},{"g":"a","v":2},{"g":"a","v":3},{"g":"a","v":4}]},
    })]);
    let payload = common::payload(&out.charts[0]);
    let raw = &payload.quant["v"];
    assert_eq!(
        (raw.min, raw.max, raw.steps, raw.resolution),
        (1.0, 4.0, 3, Some(1.0))
    );
    let record = payload
        .derived
        .iter()
        .flat_map(|state| &state.derived)
        .find(|record| record.key == "boxplot.summaries")
        .unwrap();
    let grid: richview_chart::payload::Quant =
        serde_json::from_value(record.value["grid"].clone()).unwrap();
    assert_eq!((grid.min, grid.max), (raw.min, raw.max));
    assert!(grid.steps >= 4095);
    assert_eq!(grid.resolution, None);
    let summary = &record.value["groups"][0];
    for (field, source) in [("q1", 1.75), ("median", 2.5), ("q3", 3.25)] {
        let coordinate = &summary[field];
        assert_eq!(coordinate.as_object().unwrap().len(), 1);
        let index = coordinate["grid"]
            .as_u64()
            .expect("fractional statistics use the refined grid");
        assert!(index <= u64::from(grid.steps));
        let decoded = grid.at(index);
        let half_step = (grid.max - grid.min) / f64::from(grid.steps) / 2.0;
        assert!(
            (decoded - source).abs() <= half_step + 1e-12,
            "{field}: {decoded} vs {source}"
        );
        let text =
            richview_chart::numbers::value(source, &richview_chart::numbers::Format::default());
        assert_eq!(summary["text"][field], text);
    }
}

#[test]
fn constant_box_statistics_use_only_shown_values_when_no_position_grid_exists() {
    let source = 1.23456789;
    let out = report(&[json!({
        "type":"boxplot", "title":"Constant observations", "encoding":{"x":"g", "y":"v"},
        "data":{"inline":[{"g":"a","v":source},{"g":"a","v":source}]},
    })]);
    let payload = common::payload(&out.charts[0]);
    assert!(!payload.quant.contains_key("v"));
    let record = payload
        .derived
        .iter()
        .flat_map(|state| &state.derived)
        .find(|record| record.key == "boxplot.summaries")
        .unwrap();
    assert!(record.value["grid"].is_null());
    let summary = &record.value["groups"][0];
    for field in [
        "min",
        "q1",
        "median",
        "q3",
        "max",
        "lower_whisker",
        "upper_whisker",
    ] {
        assert_eq!(summary[field], json!({"shown":1.23}), "{field}");
    }
    assert!(!out.html.contains("1.23456789"));
}

#[test]
fn core_embeds_each_filters_exact_histogram_tallies() {
    let data = json!([
        {"region":"North", "v":1}, {"region":"North", "v":2},
        {"region":"North", "v":9.996}, {"region":"North", "v":11}, {"region":"North", "v":30},
        {"region":"South", "v":1}, {"region":"South", "v":2},
        {"region":"South", "v":9.996}, {"region":"South", "v":11},
        {"region":"South", "v":12}, {"region":"South", "v":25},
    ]);
    let out = report(&[json!({
        "type":"histogram", "title":"By region", "encoding":{"x":"v"},
        "data":{"inline":data}, "binning":{"edges":[0,10,20,30]},
        "controls":[{"kind":"filter", "id":"region", "field":"region", "label":"Region",
            "options":["North","South"], "initial":"North"}],
    })]);
    let payload = common::payload(&out.charts[0]);
    assert_eq!(payload.derived.len(), 2);
    assert_ne!(payload.derived[0].state, payload.derived[1].state);
    let tallies: Vec<&Value> = payload
        .derived
        .iter()
        .map(|s| {
            &s.derived
                .iter()
                .find(|d| d.key == "histogram.tally")
                .unwrap()
                .value
        })
        .collect();
    // The 9.996 observation stays below 10 when tallied, although its shown value is 10.
    assert_eq!(tallies[0], &json!([[[3], [1], [1]], 5, 0]));
    assert_eq!(tallies[1], &json!([[[3], [2], [1]], 6, 0]));
}

#[test]
fn numeric_zoom_positions_never_gain_a_deeper_grid_than_author_precision_allows() {
    let out = report(&[json!({
        "type":"line", "title":"Numeric x", "encoding":{"x":"x", "y":"y"},
        "data":{"inline":[{"x":1.2345678901,"y":1},{"x":2.3456789012,"y":3},
            {"x":3.4567890123,"y":2},{"x":9.8765432109,"y":4}]},
    })]);
    let payload = common::payload(&out.charts[0]);
    assert!(payload.quant["x"].steps <= 4095, "{:#?}", payload.quant);
}

#[test]
fn a_value_shown_coarser_than_a_refined_grid_travels_no_finer_than_it_shows() {
    // Whole numbers under 10,000 show their units (FEA-CH-22), so this y grid refines to steps
    // of one; from 10,000 a value shows three digits, and its position may say no more than its
    // shown value or the base grid's step does (TYP-36, FEA-CH-37).
    let ys = [650.08, 9501.17, 10_001.234_6, 10_045.67, 33_333.3, 65_501.2];
    let data: Vec<Value> = ys
        .iter()
        .enumerate()
        .map(|(i, y)| json!({"x": i, "y": y}))
        .collect();
    let out = report(&[json!({
        "type": "scatter", "title": "Income", "encoding": {"x": "x", "y": "y"},
        "data": {"inline": data},
    })]);
    let payload = common::payload(&out.charts[0]);
    let grid = &payload.quant["y"];
    assert_eq!((grid.min, grid.max, grid.steps), (0.0, 66_000.0, 66_000));
    let base = (grid.max - grid.min) / 4095.0;
    let at = payload.columns.iter().position(|c| c == "y").unwrap();
    for ((row, shown), y) in payload.rows.iter().zip(&payload.shown["y"]).zip(ys) {
        let p = grid.at(row[at].as_u64().expect("a grid step"));
        let shown = shown.as_f64().unwrap();
        let off_base = ((p - grid.min) / base - ((p - grid.min) / base).round()).abs() * base;
        assert!(
            p == shown || off_base <= 0.5,
            "{y} travels as {p}, finer than its shown {shown} and the base step {base:.3}"
        );
        assert!((p - y).abs() <= base, "{y} travels as {p}");
    }
    assert_eq!(payload.shown["y"][2], json!(10_000));
    assert_eq!(grid.at(payload.rows[2][at].as_u64().unwrap()), 10_000.0);
}

#[test]
fn network_layout_coordinates_are_bounded_at_six_digits_while_weights_keep_shown_precision() {
    let out = report(&[json!({
        "type":"network", "title":"Weighted network",
        "encoding":{"source":"a", "target":"b", "y":"weight"},
        "data":{"inline":[{"a":"A","b":"B","weight":1.23456789},
            {"a":"B","b":"C","weight":2.3456789},{"a":"C","b":"A","weight":3.456789}]},
    })]);
    let payload = common::payload(&out.charts[0]);
    assert_eq!(
        payload.shown["weight"],
        vec![json!(1.23), json!(2.35), json!(3.46)]
    );
    let positions = payload
        .derived
        .iter()
        .flat_map(|s| &s.derived)
        .find(|d| d.key == "network.positions")
        .expect("network placement");
    let positions: Vec<[f64; 2]> = serde_json::from_value(positions.value.clone()).unwrap();
    assert_eq!(positions.len(), 3);
    let g = geometry(&out.charts[0], 832.0);
    let aspect = g.panels[0].plot.w / g.panels[0].plot.h;
    for [x, y] in positions {
        // The layout uses a normalized box whose x span is its fixed aspect and y span is one.
        assert!(x.is_finite() && (0.0..=aspect + 1e-5).contains(&x), "x={x}");
        assert!(y.is_finite() && (0.0..=1.0).contains(&y), "y={y}");
        for v in [x, y] {
            assert!(
                (v - richview_chart::numbers::round_sig(v, 6)).abs() < 1e-12,
                "{v}"
            );
        }
    }
}

#[test]
fn a_narrow_range_keeps_its_steps_and_equal_values_stay_equal() {
    let data: Vec<Value> = (0..40)
        .map(|i| {
            let y = 100.6 + f64::from(i) * 0.02;
            json!({"x": format!("c{i}"), "lo": y, "hi": y, "y": y})
        })
        .collect();
    let out = report(&[json!({
        "type": "interval", "title": "Range",
        "encoding": {"x": "x", "lower": "lo", "upper": "hi", "y": "y"},
        "data": {"inline": data},
    })]);
    let payload = &out.charts[0];
    let at = |name: &str| {
        payload["columns"]
            .as_array()
            .unwrap()
            .iter()
            .position(|c| c == name)
            .unwrap()
    };
    let rows = payload["rows"].as_array().unwrap();
    let distinct: std::collections::BTreeSet<String> =
        rows.iter().map(|r| r[at("y")].to_string()).collect();
    assert!(
        distinct.len() > 30,
        "{} distinct values survive",
        distinct.len()
    );
    for r in rows {
        assert_eq!(
            r[at("lo")],
            r[at("hi")],
            "one grid serves lower, upper and y"
        );
    }
}

#[test]
fn nothing_numeric_that_is_plotted_ships_exact() {
    let data = json!([
        {"k": "a", "t": 90.987654321, "v": 70.123456789},
        {"k": "b", "t": 85.123456789, "v": 66.987654321},
    ]);
    let out = report(&[json!({
        "type": "bullet", "title": "Targets",
        "encoding": {"x": "k", "y": "v", "target": "t"},
        "data": {"inline": data},
    })]);
    for source in [
        "90.987654321",
        "85.123456789",
        "70.123456789",
        "66.987654321",
    ] {
        assert!(!out.html.contains(source), "{source} reached the HTML");
    }
}

#[test]
fn a_chart_with_controls_embeds_the_initial_states_readout_text_only() {
    let data = json!([
        {"c": "A", "y": 60000000, "year": "2000"},
        {"c": "B", "y": 82000000, "year": "2000"},
        {"c": "A", "y": 61000000, "year": "2001"},
        {"c": "B", "y": 83000000, "year": "2001"},
    ]);
    let out = report(&[json!({
        "type": "bar", "title": "Population",
        "encoding": {"x": "c", "y": "y"}, "data": {"inline": data},
        "controls": [{"kind": "playback", "id": "year", "field": "year", "label": "Year"}],
    })]);
    let text = out.charts[0]["text"].as_array().unwrap();
    let states: std::collections::BTreeSet<String> =
        text.iter().map(|t| t["state"].to_string()).collect();
    assert_eq!(states.len(), 1, "no per-state text: {states:?}");
    assert!(text.iter().any(|t| t.to_string().contains("61 million")));
}

#[test]
fn a_decimal_detail_column_ships_at_the_authors_precision() {
    let data = json!([
        {"k": "a", "y": 1, "note": "12.345678901234567890"},
        {"k": "b", "y": 2, "note": "3.14159265358979323846"},
    ]);
    let out = report(&[json!({
        "type": "bar", "title": "Notes",
        "encoding": {"x": "k", "y": "y", "detail": ["note"]},
        "data": {"inline": data},
    })]);
    assert!(
        !out.html.contains("12.3456789"),
        "digits beyond the author's reached the HTML"
    );
    assert!(!out.html.contains("3.14159265"));
    assert!(out.html.contains("12.3") || out.html.contains("3.14"));
}

#[test]
fn numeric_details_keep_shown_precision_without_position_grids() {
    let out = report(&[json!({
        "type":"bar", "title":"Notes", "encoding":{"x":"k", "y":"y", "detail":["note"]},
        "data":{"inline":[{"k":"A","y":1,"note":12.3456},{"k":"B","y":2,"note":12.3467}]},
    })]);
    let payload = common::payload(&out.charts[0]);
    assert!(
        !payload.quant.contains_key("note"),
        "a context value has no position scale"
    );
    let at = payload.columns.iter().position(|c| c == "note").unwrap();
    assert!(payload.rows.iter().all(|r| r[at] == json!(12.3)));
    assert_eq!(
        (payload.quant["y"].min, payload.quant["y"].max),
        (1.0, 2.0),
        "context values do not enlarge a position scale"
    );
}

#[test]
fn the_payload_is_the_same_for_every_style() {
    let chart = json!({
        "type": "bar", "title": "Styles",
        "encoding": {"x": "k", "y": "y"},
        "data": {"inline": [{"k": "a", "y": 1.2345}, {"k": "b", "y": 2.3456}, {"k": "c", "y": 99.5}]},
    });
    let payloads: Vec<Value> = ["sleek", "air", "scientific"]
        .iter()
        .map(|style| {
            report_with(std::slice::from_ref(&chart), vec![(*style).to_owned()]).charts[0].clone()
        })
        .collect();
    let data = |p: &Value| json!({"columns": p["columns"], "rows": p["rows"], "quant": p["quant"], "shown": p["shown"], "text": p["text"], "summary": p["summary"]});
    assert_eq!(data(&payloads[0]), data(&payloads[1]));
    assert_eq!(data(&payloads[0]), data(&payloads[2]));
}

/// A 1,650-row playback chart (the gapminder shape) keeps the payload linear in its data: no text
/// per control state, one row of steps and shown values per source row.
#[test]
fn a_1650_row_playback_chart_embeds_linearly() {
    let data: Vec<Value> = (0..1650)
        .map(|i| {
            let country = i % 150;
            let year = 1950 + (i / 150) * 6;
            json!({
                "country": format!("Country {country}"),
                "year": year.to_string(),
                "income": 500.0 + f64::from(country) * 311.37 + f64::from(year - 1950) * 17.113,
                "life": 40.0 + f64::from(country) * 0.231 + f64::from(year - 1950) * 0.3711,
                "population": 1_000_000.0 + f64::from(country) * 123_456.789,
            })
        })
        .collect();
    let out = report(&[json!({
        "type": "scatter", "title": "Playback",
        "encoding": {"x": "income", "y": "life", "size": "population", "color": "country"},
        "data": {"inline": data},
        "controls": [{"kind": "playback", "id": "year", "field": "year", "label": "Year"}],
    })]);
    let figure = figures(&out.html)[0];
    assert!(
        figure.len() < 600_000,
        "the payload is {} bytes for 1,650 rows",
        figure.len()
    );
}

#[test]
fn numeric_identifiers_travel_as_written() {
    let bars = report(&[json!({
        "type":"bar", "title":"Numbered categories",
        "encoding":{"x":"category", "color":"series", "y":"value"},
        "data":{"inline":[{"category":1000,"series":1,"value":12},
            {"category":1001,"series":2,"value":13}]},
    })]);
    let payload = common::payload(&bars.charts[0]);
    assert!(!payload.quant.contains_key("category") && !payload.quant.contains_key("series"));
    let rows = payload.objects();
    assert_eq!(rows[0]["category"], json!(1000));
    assert_eq!(rows[1]["category"], json!(1001));
    assert_eq!(rows[0]["series"], json!(1));
    assert_eq!(rows[1]["series"], json!(2));
    let tree = report(&[json!({
        "type": "tree", "title": "Org",
        "encoding": {"label": "id", "parent": "boss"},
        "data": {"inline": [
            {"id": 1000, "boss": null},
            {"id": 1001, "boss": 1000},
            {"id": 1002, "boss": 1000},
        ]},
    })]);
    assert!(
        !tree.diagnostics.iter().any(|d| d.level == "error"),
        "{:?}",
        tree.diagnostics
    );
    assert!(
        !tree.html.contains("1000.0"),
        "an id was gridded into a float"
    );
    let sankey = report(&[json!({
        "type": "sankey", "title": "Flow",
        "encoding": {"source": "from", "target": "to", "y": "n"},
        "data": {"inline": [
            {"from": 1, "to": 2, "n": 5},
            {"from": 2, "to": 3, "n": 4},
        ]},
    })]);
    assert!(
        !sankey.diagnostics.iter().any(|d| d.level == "error"),
        "{:?}",
        sankey.diagnostics
    );
    assert!(
        !sankey.html.contains("\"1.0\""),
        "a node id read as a float"
    );
}

#[test]
fn unrelated_scales_each_get_their_own_grid() {
    let data: Vec<Value> = (0..30)
        .map(|i| {
            json!({"x": i, "y": 100.0 + f64::from(i) * 0.01, "z": f64::from(i) * 1e6, "size": 3.0 + f64::from(i % 5)})
        })
        .collect();
    let out = report(&[json!({
        "type": "scatter", "title": "Scales",
        "encoding": {"x": "x", "y": "y", "size": "size"},
        "data": {"inline": data},
    })]);
    let quant = out.charts[0]["quant"].as_object().unwrap();
    let (y, size) = (&quant["y"], &quant["size"]);
    assert_ne!(y, size, "y and size have separate grids");
    assert!(
        y["max"].as_f64().unwrap() < 101.0,
        "y keeps its own narrow domain: {y}"
    );
}

#[test]
fn a_wide_chart_carries_the_full_width_class() {
    let chart = |wide: bool| {
        report(&[json!({
            "type": "bar", "title": "Wide", "wide": wide,
            "encoding": {"x": "k", "y": "y"},
            "data": {"inline": [{"k": "a", "y": 1}, {"k": "b", "y": 2}]},
        })])
    };
    let wide = chart(true);
    let normal = chart(false);
    assert!(wide.html.contains("class=\"rv-chart rv-chart--full\""));
    assert!(!normal.html.contains("rv-chart--full\""));
    for rendered in [&normal, &wide] {
        assert!(
            !rendered.html.contains("measure-wide-blocks"),
            "block widths use wide, with no retired layout switch or CSS"
        );
    }
}

/// `auto_annotations` is validated vocabulary that reaches the runtime's layout: on by default,
/// and `false` draws no automatic value (FEA-CH-28).
#[test]
fn auto_annotations_is_on_by_default_and_the_authors_false_reaches_the_runtime() {
    let chart = |switch: Option<bool>| {
        let mut spec = json!({
            "type": "bar", "title": "Amounts",
            "encoding": {"x": "k", "y": "y"},
            "data": {"inline": [{"k": "a", "y": 28}, {"k": "b", "y": 68}]},
        });
        if let Some(on) = switch {
            spec["auto_annotations"] = json!(on);
        }
        let out = report(&[spec]);
        assert!(
            out.diagnostics.is_empty(),
            "{switch:?}: {:?}",
            out.diagnostics
        );
        let g = geometry(&out.charts[0], 720.0);
        texts(&g, Some(Role::Label))
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(chart(None), ["28", "68"]);
    assert_eq!(chart(Some(true)), ["28", "68"]);
    assert!(chart(Some(false)).is_empty());
    let out = report(&[json!({
        "type": "bar", "title": "Amounts", "auto_annotations": "off",
        "encoding": {"x": "k", "y": "y"},
        "data": {"inline": [{"k": "a", "y": 28}]},
    })]);
    assert!(
        out.diagnostics.iter().any(|d| d.code == "block.schema"),
        "only true or false: {:?}",
        out.diagnostics
    );
}

#[test]
fn a_heatmap_with_numeric_rows_keeps_them_as_written_and_n_a_does_not_leak() {
    let mut data: Vec<Value> = Vec::new();
    for row in [2, 7, 12] {
        for col in ["a", "b"] {
            data.push(json!({"row": row, "col": col, "v": f64::from(row) * 1.234_567_89}));
        }
    }
    let out = report(&[json!({
        "type": "heatmap", "title": "Rows",
        "encoding": {"x": "col", "y": "row", "color": "v"},
        "data": {"inline": data},
    })]);
    assert!(
        !out.html.contains("2.0002"),
        "a categorical row was gridded"
    );
    assert!(out.html.contains("[\"a\",2,"), "rows keep their values");
    let detail = report(&[json!({
        "type": "bar", "title": "Missing",
        "encoding": {"x": "k", "y": "y", "detail": ["note"]},
        "data": {"inline": [
            {"k": "a", "y": 1, "note": "12.3456789012345"},
            {"k": "b", "y": 2, "note": "n/a"},
        ]},
    })]);
    assert!(
        !detail.html.contains("12.3456789"),
        "a decimal beside n/a shipped exact"
    );
}

/// A grid is never finer than both the last digit the author shows and 1/4,096 of the domain, so
/// no zoomable x decodes to more digits than the author chose (TYP-36, FEA-CH-37).
#[test]
fn a_zoomable_x_decodes_to_no_more_digits_than_the_author_shows() {
    let xs = [0.0123, 50.5, 99.9];
    let data: Vec<Value> = xs.iter().map(|x| json!({"x": x, "y": 1})).collect();
    let out = report(&[json!({
        "type": "scatter", "title": "Digits",
        "encoding": {"x": "x", "y": "y"}, "data": {"inline": data},
    })]);
    let payload = &out.charts[0];
    let grid = &payload["quant"]["x"];
    let (min, max, steps) = (
        grid["min"].as_f64().unwrap(),
        grid["max"].as_f64().unwrap(),
        grid["steps"].as_u64().unwrap(),
    );
    assert!((4000..=4096).contains(&steps) || steps > 4096, "{grid}");
    // The finest shown value has four decimals, so no step is finer than 1e-4 unless 4,096 steps
    // across the domain already are.
    let step = (max - min) / steps as f64;
    assert!(step >= 1e-4 - 1e-12 || steps <= 4096, "{grid}");
    let at = payload["columns"]
        .as_array()
        .unwrap()
        .iter()
        .position(|c| c == "x")
        .unwrap();
    for (row, shown) in payload["rows"]
        .as_array()
        .unwrap()
        .iter()
        .zip(["0.0123", "50.5", "99.9"])
    {
        let decoded = min + row[at].as_u64().unwrap() as f64 * (max - min) / steps as f64;
        let text = format!("{:.4}", decoded);
        assert_eq!(
            text.trim_end_matches('0').trim_end_matches('.'),
            shown,
            "{decoded} decodes past the author's digits"
        );
    }
}

/// What the exact layout derived travels once per control state, so a filter option or a playback
/// frame never replays another state's bins, order or extent (TYP-36, review 1k blocker 1).
#[test]
fn derived_structures_are_embedded_per_control_state() {
    let data = json!([
        {"c": "A", "y": 60, "year": "2000"},
        {"c": "B", "y": 82, "year": "2000"},
        {"c": "A", "y": 6100, "year": "2001"},
        {"c": "B", "y": 83, "year": "2001"},
    ]);
    let out = report(&[json!({
        "type": "bar", "title": "Population",
        "encoding": {"x": "c", "y": "y"}, "data": {"inline": data},
        "controls": [{"kind": "playback", "id": "year", "field": "year", "label": "Year"}],
    })]);
    let derived = out.charts[0]["derived"]
        .as_array()
        .expect("derived per state");
    let states: Vec<String> = derived.iter().map(|d| d["state"].to_string()).collect();
    let unique: std::collections::BTreeSet<&String> = states.iter().collect();
    assert!(
        states.len() >= 2 && unique.len() == states.len(),
        "{states:?}"
    );
}
