//! The per-type contract of FEA-CH-75 that every chart type shares: a type's test file calls
//! [`contract`] on its golden fixture and the edge fixtures, then adds the checks that are only
//! its own.
#![allow(dead_code)]

use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Cmd, Geometry, Item, Observation, Role, Targeting};
use richview_chart::hit;
use richview_chart::layout::{ChartTheme, Options, layout};
use serde_json::Value;

/// The widths figures occupy (FEA-CH-64).
pub const WIDTHS: [f64; 3] = [320.0, 390.0, 1440.0];

pub fn draw_at(spec: &Value, rows: &[Value], width: f64) -> Geometry {
    let block = ChartBlock::from_value(spec).expect("a chart block");
    layout(
        &block,
        rows,
        &ChartTheme::default(),
        &Options {
            width,
            ..Options::default()
        },
    )
}

pub fn draw(spec: &Value, rows: &[Value]) -> Geometry {
    draw_at(spec, rows, 640.0)
}

pub fn observations(g: &Geometry) -> Vec<&Observation> {
    g.panels.iter().flat_map(|p| &p.observations).collect()
}

pub fn marks(g: &Geometry) -> Vec<&Item> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| i.role() == Role::Mark)
        .collect()
}

pub fn check<'a>(g: &'a Geometry, kind: &str) -> Option<&'a richview_chart::geometry::Check> {
    g.checks.iter().find(|c| c.kind == kind)
}

pub fn line<'a>(o: &'a Observation, name: &str) -> Option<&'a str> {
    o.readout
        .lines
        .iter()
        .find(|l| l.name == name)
        .map(|l| l.value.as_str())
}

/// The first item whose coordinates are not finite.
pub fn nonfinite(g: &Geometry) -> Option<&Item> {
    let ok = |v: f64| v.is_finite();
    g.items().find(|i| !match i {
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

/// `rows` as the report runtime sees them (TYP-36): the numeric columns the encoding names, each
/// scale on a 4,096-step grid of its own. The value channels (y, lower, upper, open, high, low,
/// close, target) share one scale and so do a Gantt's start and end; x, size, z, angle, line,
/// pivot values, a heatmap's colour and every column of `dimensions` have one each.
pub fn quantized(spec: &Value, rows: &[Value]) -> Vec<Value> {
    use richview_chart::payload::{QUANT_STEPS, Quant};
    use std::collections::BTreeMap;
    let mut axes: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    let kind = spec.get("type").and_then(Value::as_str).unwrap_or_default();
    if let Some(encoding) = spec.get("encoding").and_then(Value::as_object) {
        // Only value channels travel on a grid; ids, categories, facets and details keep their
        // source text. An x is a value on the types that place marks along a number line.
        let numeric_x = [
            "line",
            "area",
            "scatter",
            "histogram",
            "density",
            "ecdf",
            "dotplot",
            "bin2d",
            "hexbin",
            "contour",
            "vector",
            "surface",
            "scatter3d",
            "strip",
            "violin",
            "qq",
        ];
        for (channel, field) in encoding {
            let axis = match channel.as_str() {
                "x" if numeric_x.contains(&kind) => "x".to_owned(),
                "y" | "lower" | "upper" | "open" | "high" | "low" | "close" | "target" => "value".to_owned(),
                "start" | "end" => "span".to_owned(),
                "size" | "z" | "angle" | "line" | "values" | "dimensions" => channel.clone(),
                "color" if kind == "heatmap" => channel.clone(),
                _ => continue,
            };
            let names: Vec<&str> = match field {
                Value::String(name) => vec![name.as_str()],
                Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
                _ => vec![],
            };
            for name in names {
                let numeric = rows.iter().filter_map(|r| r.get(name)).all(Value::is_number)
                    && rows.iter().any(|r| r.get(name).is_some());
                if numeric {
                    // Each `dimensions` column is a scale of its own.
                    let key = if axis == "dimensions" {
                        format!("dimensions:{name}")
                    } else {
                        axis.clone()
                    };
                    axes.entry(key).or_default().push(name);
                }
            }
        }
    }
    let mut out = rows.to_vec();
    for (_, columns) in axes {
        let numbers = || {
            rows.iter()
                .flat_map(|r| columns.iter().filter_map(|c| r.get(*c)))
                .filter_map(Value::as_f64)
        };
        let resolution = numbers().all(|v| v.fract() == 0.0).then_some(1.0);
        let Some(grid) = Quant::over(numbers()).and_then(|q| q.on_grid(QUANT_STEPS, resolution)) else {
            continue;
        };
        for row in &mut out {
            for column in &columns {
                if let Some(v) = row.get(*column).and_then(Value::as_f64) {
                    row[*column] = serde_json::json!(grid.snap(v));
                }
            }
        }
    }
    out
}

/// What the report runtime draws (a layout from the grid rows, handed what the exact-row layout
/// derived and the texts it wrote) is the figure `check` and the PDF verified: the same items,
/// within a fraction of a pixel, and the same observations (TYP-36).
pub fn runtime_matches_native(spec: &Value, rows: &[Value]) {
    let grid = quantized(spec, rows);
    let block = ChartBlock::from_value(spec).expect("a chart block");
    for width in WIDTHS {
        let native = draw_at(spec, rows, width);
        let options = Options {
            width,
            derived: native.derived.clone(),
            ..Options::default()
        };
        let mut runtime = layout(&block, &grid, &ChartTheme::default(), &options);
        let at = format!("{} at {width} px", native.kind);
        runtime.set_mark_texts(&native.mark_texts());
        assert_eq!(native.failure, runtime.failure, "{at}: failure");
        assert_eq!(native.panels.len(), runtime.panels.len(), "{at}: panels");
        for (i, (a, b)) in native.panels.iter().zip(&runtime.panels).enumerate() {
            assert_eq!(
                a.observations.len(),
                b.observations.len(),
                "{at}: panel {i} observations"
            );
            let seen = |p: &richview_chart::geometry::Panel| p.items.iter().filter(|i| visible(i)).count();
            let describe = |p: &richview_chart::geometry::Panel| {
                p.items
                    .iter()
                    .filter(|i| visible(i))
                    .map(|i| match i {
                        Item::Text(t) => format!("{:?} {:?}", t.role, t.text),
                        _ => format!("{:?}", i.role()),
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                seen(a),
                seen(b),
                "{at}: panel {i} items\nnative {:?}\nruntime {:?}",
                describe(a),
                describe(b)
            );
        }
        let (a, b): (Vec<_>, Vec<_>) = (
            native.items().filter(|i| visible(i)).collect(),
            runtime.items().filter(|i| visible(i)).collect(),
        );
        assert_eq!(a.len(), b.len(), "{at}: items");
        for (n, (x, y)) in a.iter().zip(&b).enumerate() {
            match (x.bounds(), y.bounds()) {
                (Some(p), Some(q)) => {
                    let off = [p.x - q.x, p.y - q.y, p.w - q.w, p.h - q.h]
                        .into_iter()
                        .fold(0.0f64, |m, d| m.max(d.abs()));
                    assert!(off <= 1.5, "{at}: item {n} moved {off:.2} px: {x:?} against {y:?}");
                }
                (None, None) => {}
                _ => panic!("{at}: item {n} differs: {x:?} against {y:?}"),
            }
            if let (Item::Text(p), Item::Text(q)) = (x, y) {
                assert_eq!(p.text, q.text, "{at}: text {n}");
            }
        }
        for (i, (a, b)) in native.panels.iter().zip(&runtime.panels).enumerate() {
            for (k, (p, q)) in a.observations.iter().zip(&b.observations).enumerate() {
                assert_eq!(p.row, q.row, "{at}: panel {i} observation {k} row");
                assert_eq!(p.series, q.series, "{at}: panel {i} observation {k} series");
            }
        }
    }
}

/// Whether a reader can see the item: a mark thinner than a tenth of a pixel is a zero (a value
/// the grid rounds to nothing is drawn as no mark, where the exact value drew a hairline).
fn visible(item: &Item) -> bool {
    match item {
        Item::Rect { rect, role, .. } if *role == Role::Mark => rect.w >= 0.1 && rect.h >= 0.1,
        _ => true,
    }
}

/// What every type promises on `spec` and `rows` at 320, 390 and 1440 px: it draws, every
/// coordinate is finite, nothing paints outside the figure, no text is clipped or collides, every
/// observation can be reached by pointer and by keyboard and is announced, and the chart names
/// itself for screen readers.
pub fn contract(spec: &Value, rows: &[Value]) {
    runtime_matches_native(spec, rows);
    for width in WIDTHS {
        let g = draw_at(spec, rows, width);
        let at = format!("{} at {width} px", g.kind);
        assert!(g.failure.is_none(), "{at} failed: {:?}", g.failure);
        assert!(
            nonfinite(&g).is_none(),
            "{at} has a non-finite coordinate: {:?}",
            nonfinite(&g)
        );
        assert!(!g.summary.trim().is_empty(), "{at} has no summary");
        let figure = richview_chart::geometry::Rect::new(0.0, 0.0, g.width, g.height);
        for item in g.items() {
            if let Some(b) = item.bounds() {
                assert!(
                    b.inside(&figure, 1.0),
                    "{at}: {item:?} paints outside the figure {figure:?}"
                );
            }
        }
        for kind in ["clipping", "label-collision"] {
            assert!(check(&g, kind).is_none(), "{at}: {kind}: {:?}", check(&g, kind));
        }
        // Hover bisects a nearest-x panel's observations, so they are in x order (PRF-19).
        for panel in g.panels.iter().filter(|p| p.targeting == Targeting::NearestX) {
            let xs: Vec<f64> = panel.observations.iter().map(|o| o.x).collect();
            assert!(
                xs.is_sorted() || xs.iter().rev().is_sorted(),
                "{at}: a nearest-x panel's observations are in x order"
            );
        }
        let order = hit::order(&g);
        assert_eq!(
            order.len(),
            observations(&g).len(),
            "{at}: keyboard order covers every observation"
        );
        for t in &order {
            assert!(hit::announce(&g, *t).is_some(), "{at}: {t:?} is announced");
            let o = &g.panels[t.panel].observations[t.observation];
            if let Some(area) = o.area {
                // Reached at its anchor or at the centre of its box (a shape need not fill its box).
                let found = hit::at(&g, o.at).or_else(|| hit::at(&g, area.center()));
                assert!(
                    found.is_some(),
                    "{at}: {:?} is not reachable by pointer",
                    o.readout.heading
                );
            }
        }
    }
}

/// A chart with no rows, or rows with nothing drawable, never disappears (FEA-CH-18).
pub fn stays_visible(spec: &Value, rows: &[Value]) {
    let g = draw(spec, rows);
    assert!(g.failure.is_some(), "{} draws a reason when it cannot draw", g.kind);
    assert!(!g.panels.is_empty());
}

/// Write the chart's vector rendering to `$RICHVIEW_GALLERY_OUT/<name>-<width>.svg` for looking
/// at it; does nothing unless the variable is set.
pub fn gallery(name: &str, spec: &Value, rows: &[Value]) {
    let Ok(dir) = std::env::var("RICHVIEW_GALLERY_OUT") else {
        return;
    };
    std::fs::create_dir_all(&dir).expect("gallery dir");
    for width in [390.0, 720.0] {
        let g = draw_at(spec, rows, width);
        std::fs::write(
            format!("{dir}/{name}-{width}.svg"),
            richview_chart::vector::svg(&g, &ChartTheme::default()),
        )
        .expect("write");
    }
}
