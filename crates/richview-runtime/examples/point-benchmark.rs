//! FEA-CH-17 scale measurements: layout, nearest hover, WebGL mesh and vector bytes are timed
//! separately. Run in release with --samples 100; browser paint and upload are measured by
//! scripts/verify-point-modes.mjs --perf. No budget is inferred for a new fixture size.
use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Geometry, Item, Role};
use richview_chart::layout::{ChartTheme, Options, Prepared, layout_prepared};
use richview_chart::{hit, vector};
use richview_runtime::mesh;
use serde_json::{Value, json};
use std::{hint::black_box, time::Instant};

fn stats(mut samples: Vec<f64>) -> Value {
    samples.sort_by(f64::total_cmp);
    let at = |p: f64| samples[((samples.len() as f64 * p).ceil() as usize).saturating_sub(1)];
    json!({"samples":samples.len(),"medianMs":at(0.5),"p95Ms":at(0.95),"maxMs":samples.last()})
}

fn time<T>(work: impl FnOnce() -> T) -> (T, f64) {
    let started = Instant::now();
    let result = black_box(work());
    (result, started.elapsed().as_secs_f64() * 1000.0)
}

fn positions(g: &Geometry) -> usize {
    g.items()
        .filter(|i| i.role() == Role::Mark)
        .map(|i| match i {
            Item::Line { path, .. } => path.cmds.len(),
            Item::Point { .. } => 1,
            _ => 0,
        })
        .sum()
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let samples = args
        .windows(2)
        .find(|a| a[0] == "--samples")
        .map_or(100, |a| a[1].parse::<usize>().expect("a sample count"));
    assert!(samples > 0);
    if let Some(path) = args.windows(2).find(|a| a[0] == "--payload").map(|a| &a[1]) {
        let payload = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let chart = richview_runtime::payload::Chart::from_payload(payload).unwrap();
        let mut results = Vec::new();
        for width in [390.0, 1280.0] {
            let mut frames = Vec::new();
            let first = chart.geometry(width, &[], None);
            let resized = chart.geometry(width - 1.0, &[], None);
            let shared = std::rc::Rc::ptr_eq(
                &first.panels[0].observations[0].readout,
                &resized.panels[0].observations[0].readout,
            );
            for sample in 0..samples + 5 {
                let (geometry, ms) =
                    time(|| chart.geometry(width - (sample % 2) as f64, &[], None));
                assert!(geometry.failure.is_none());
                if sample >= 5 {
                    frames.push(ms);
                }
            }
            results.push(json!({"width":width,"chartGeometry":stats(frames),"reusedReadout":shared,"markTexts":first.mark_texts().len()}));
        }
        println!(
            "{}",
            json!({"scope":"Real runtime payload, alternating widths, including grid/shown precision and embedded exact readouts; excludes mesh/upload/paint", "results":results})
        );
        return;
    }
    let theme = ChartTheme::default();
    let mut results = Vec::new();
    for count in [1_000, 10_000, 100_000] {
        let rows: Vec<_> = (0..count)
            .map(|i| json!({"x":i,"y":(i * 37 % 101) + 1}))
            .collect();
        for kind in ["line", "area", "scatter"] {
            let modes: &[&str] = if kind == "scatter" {
                &["default", "all"]
            } else {
                &["default", "all", "m4", "band"]
            };
            for mode in modes {
                for curve in if kind == "scatter" {
                    &["linear"][..]
                } else {
                    &["linear", "monotone"][..]
                } {
                    let mut value = json!({"type":kind,"title":"Scale measurement",
                        "encoding":{"x":"x","y":"y"},"style":{"curve":curve}});
                    if *mode != "default" {
                        value["style"]["points"] = json!(mode);
                    }
                    let block = ChartBlock::from_value(&value).unwrap();
                    for width in [390.0, 1280.0] {
                        let base = Options {
                            width,
                            ..Options::default()
                        };
                        let prepared = Prepared::default();
                        let (g, cold_ms) =
                            time(|| layout_prepared(&block, &rows, &theme, &base, &prepared));
                        let ceiling = richview_chart::limits::max(kind, "all-points observations");
                        assert!(g.failure.is_none(), "{kind} {count}: {:?}", g.failure);
                        if *mode == "all" || *mode == "default" && count <= ceiling {
                            assert_eq!(positions(&g), count);
                        } else if *mode == "default" {
                            assert!(g.checks.iter().any(|c| c.details["automatic"] == true));
                        }
                        let svg_bytes = vector::svg(&g, &theme).len();
                        let initial_mesh = mesh::build(&g, &mesh::State::default(), 1.0);
                        let mesh_bytes = initial_mesh.vertices.len() * size_of::<f32>();
                        drop(initial_mesh);
                        let mut layouts = Vec::new();
                        let mut zooms = Vec::new();
                        let mut meshes = Vec::new();
                        let mut hovers = Vec::new();
                        for sample in 0..samples + 5 {
                            let (_, layout_ms) =
                                time(|| layout_prepared(&block, &rows, &theme, &base, &prepared));
                            let zoom = Options {
                                zoom: Some([count as f64 * 0.4, count as f64 * 0.5]),
                                ..base.clone()
                            };
                            let (_, zoom_ms) =
                                time(|| layout_prepared(&block, &rows, &theme, &zoom, &prepared));
                            let (_, mesh_ms) =
                                time(|| mesh::build(&g, &mesh::State::default(), 1.0));
                            let plot = g.panels[0].plot;
                            let x = plot.x + plot.w * (sample % 100) as f64 / 100.0;
                            let (_, hover_ms) = time(|| hit::at(&g, [x, plot.y + plot.h * 0.5]));
                            if sample >= 5 {
                                layouts.push(layout_ms);
                                zooms.push(zoom_ms);
                                meshes.push(mesh_ms);
                                hovers.push(hover_ms);
                            }
                        }
                        results.push(json!({"kind":kind,"curve":curve,"points":mode,"count":count,"width":width,"ceiling":ceiling,
                        "coldPreparationAndLayoutMs":cold_ms,"fullLayout":stats(layouts),"zoomLayout":stats(zooms),
                        "mesh":stats(meshes),"hover":stats(hovers),"svgBytes":svg_bytes,"meshVertexBytes":mesh_bytes,
                        "positions":positions(&g),"checks":g.checks}));
                        eprintln!(
                            "point-benchmark {}",
                            results.last().expect("a recorded case")
                        );
                    }
                }
            }
        }
    }
    println!(
        "{}",
        json!({"scope":"Prepared native geometry and WebGL mesh; excludes parsing, Wasm start, GPU upload, browser paint and serialization. Five warmups per case. New workload sizes do not inherit fixture-specific budgets.","results":results})
    );
}
