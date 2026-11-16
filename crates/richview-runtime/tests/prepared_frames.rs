//! A chart walked the way a reader walks it draws each frame as a chart that lays out only that
//! frame draws (PRF-19): the preparations and the text layout the runtime keeps between frames
//! change no geometry, payload text included. The walk takes three widths, zoom windows inside and
//! at the ends of the data, a turned ratio, a hidden series and each control state. The charts are
//! built by core from the repository's examples, the docs' chart pages and time series.

use richview_core::document::{RenderOptions, ReportFolder, render};
use richview_runtime::payload::Chart;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every chart payload core embeds in `source`, with `data` as the report folder's other files.
fn payloads(source: &str, data: BTreeMap<String, String>) -> Vec<String> {
    let files = std::iter::once("report.rmd".to_string())
        .chain(data.keys().cloned())
        .collect();
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "corpus".into(),
            source: "report.rmd".into(),
            files,
        }),
        data,
        ..Default::default()
    };
    let Ok(rendered) = render(source, &options) else {
        return Vec::new();
    };
    rendered
        .html
        .split("data-rv-payload>")
        .skip(1)
        .map(|rest| rest[..rest.find("</script>").expect("payload end")].to_string())
        .collect()
}

/// One reader action: a width, a ratio, the hidden series and a zoom window, in a control state.
struct Frame {
    state: Vec<usize>,
    width: f64,
    ratio: Option<f64>,
    hidden: Vec<String>,
    zoom: Option<[f64; 2]>,
}

/// The frames a reader steps through, zoom windows kept inside the data as the host keeps them.
fn walk(chart: &Chart) -> Vec<Frame> {
    let first = chart.geometry(640.0, &[], None);
    let windows: Vec<Option<[f64; 2]>> = first
        .panels
        .first()
        .and_then(|p| p.x.as_ref())
        .map(|scale| {
            let [lo, hi] = scale.extent;
            [[0.25, 0.5], [0.0, 0.1], [0.9, 1.0]]
                .iter()
                .map(|[a, b]| chart.clamp_zoom([lo + (hi - lo) * a, lo + (hi - lo) * b], scale))
                .collect()
        })
        .unwrap_or_default();
    let hide: Vec<String> = first
        .series
        .first()
        .map(|s| vec![s.key.clone()])
        .unwrap_or_default();
    let initial = chart.state.clone();
    let frame = |state: &[usize], width, ratio, hidden: &[String], zoom| Frame {
        state: state.to_vec(),
        width,
        ratio,
        hidden: hidden.to_vec(),
        zoom,
    };
    let mut frames = Vec::new();
    for width in [390.0, 1280.0, 640.0] {
        frames.push(frame(&initial, width, None, &[], None));
        for zoom in &windows {
            frames.push(frame(&initial, width, None, &[], *zoom));
        }
    }
    let zoom = windows.first().copied().flatten();
    frames.push(frame(&initial, 640.0, Some(0.8), &[], None));
    frames.push(frame(&initial, 640.0, Some(0.8), &[], zoom));
    frames.push(frame(&initial, 640.0, None, &hide, None));
    for w in &windows {
        frames.push(frame(&initial, 1280.0, None, &hide, *w));
    }
    frames.push(frame(&initial, 640.0, None, &[], zoom));
    if !chart.controls.is_empty() {
        for state in chart.controls.states(4).0 {
            frames.push(frame(&state, 640.0, None, &[], None));
            frames.push(frame(&state, 640.0, None, &hide, None));
            frames.push(frame(&state, 640.0, None, &[], zoom));
        }
        frames.push(frame(&initial, 640.0, None, &[], None));
    }
    frames
}

/// Walk `json`'s chart with one chart, and lay out every frame again with a chart of its own.
fn assert_frames_match(label: &str, json: &str) {
    let Ok(pristine) = Chart::parse(json) else {
        return;
    };
    // A clone shares what its chart prepared, so the frames come from a clone that is walked
    // too, and each frame's own chart is cloned before anything is laid out.
    let mut walked = pristine.clone();
    for f in walk(&pristine.clone()) {
        let mut fresh = pristine.clone();
        fresh.set_state(f.state.clone());
        walked.set_state(f.state.clone());
        let expected = fresh.geometry_at_ratio(f.width, f.ratio, &f.hidden, f.zoom);
        assert!(
            walked.geometry_at_ratio(f.width, f.ratio, &f.hidden, f.zoom) == expected,
            "{label}: state {:?}, {} px, ratio {:?}, hidden {:?}, zoom {:?} differs from the chart laid out only there",
            f.state,
            f.width,
            f.ratio,
            f.hidden,
            f.zoom
        );
    }
}

/// The docs time series `name`, its first `rows` observations, as the docs page renders it.
fn time_series(name: &str, rows: usize) -> Vec<String> {
    let root = root();
    let source =
        std::fs::read_to_string(root.join(format!("docs/public/examples/{name}/report.rmd")))
            .expect("report");
    let csv =
        std::fs::read_to_string(root.join(format!("benchmarks/fixtures/time-series/{name}.csv")))
            .expect("csv");
    let csv: Vec<&str> = csv
        .lines()
        .skip_while(|l| l.starts_with('#'))
        .take(rows + 1)
        .collect();
    payloads(
        &source,
        [("data.csv".to_string(), csv.join("\n") + "\n")].into(),
    )
}

#[test]
fn a_walked_chart_draws_each_frame_as_a_chart_laid_out_only_there() {
    let source = r#"
```chart
type: line
title: Gaps and three series
data:
  inline:
    - { d: "2024-01-01", s: A, v: 3 }
    - { d: "2024-01-02", s: A, v: NA }
    - { d: "2024-01-03", s: A, v: 7 }
    - { d: "2024-01-05", s: A, v: 12 }
    - { d: "2024-01-01", s: B, v: 40 }
    - { d: "2024-01-03", s: B, v: 250 }
    - { d: "2024-01-04", s: B, v: 7 }
    - { d: "2024-01-01", s: C, v: 5.5 }
    - { d: "2024-01-05", s: C, v: 6.25 }
encoding: { x: d, y: v, color: s }
```

```chart
type: area
title: A stack
stack: true
data:
  inline:
    - { m: "2024-01", s: A, v: 3 }
    - { m: "2024-02", s: A, v: 7 }
    - { m: "2024-03", s: A, v: 12 }
    - { m: "2024-01", s: B, v: 40 }
    - { m: "2024-02", s: B, v: 250 }
    - { m: "2024-03", s: B, v: 7 }
encoding: { x: m, y: v, color: s }
```

```chart
type: scatter
title: Points by region
data:
  inline:
    - { region: North, x: 1.5, y: 3, n: a }
    - { region: North, x: 2.25, y: 7, n: b }
    - { region: South, x: 3.125, y: 12, n: c }
    - { region: South, x: 4, y: 40, n: d }
encoding: { x: x, y: y, color: region, label: n }
controls:
  - { kind: filter, id: r, field: region, options: [North, South] }
```

```chart
type: line
title: A playback
data:
  inline:
    - { year: 2020, t: 1, v: 3 }
    - { year: 2020, t: 2, v: 7 }
    - { year: 2021, t: 1, v: 12 }
    - { year: 2021, t: 2, v: 40 }
encoding: { x: t, y: v }
controls:
  - { kind: playback, id: yr, field: year }
```
"#;
    let charts = payloads(source, BTreeMap::new());
    assert_eq!(charts.len(), 4, "every chart of the source renders");
    for (i, json) in charts.iter().enumerate() {
        assert_frames_match(&format!("chart {i}"), json);
    }
    for json in time_series("co2-mauna-loa-monthly", 822) {
        assert_frames_match("co2", &json);
    }
}

/// Every file under `dir`, keyed by its path below the report folder.
fn folder_files(dir: &Path, prefix: &str, out: &mut BTreeMap<String, String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.map(|e| e.expect("entry").path()) {
        let name = format!(
            "{prefix}{}",
            path.file_name().expect("name").to_string_lossy()
        );
        if path.is_dir() {
            folder_files(&path, &format!("{name}/"), out);
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            out.insert(name, text);
        }
    }
}

#[test]
fn every_corpus_chart_draws_each_frame_as_a_chart_laid_out_only_there() {
    let root = root();
    let mut reports: Vec<(String, String, BTreeMap<String, String>)> = Vec::new();
    for dir in std::fs::read_dir(root.join("examples"))
        .expect("examples")
        .map(|e| e.expect("entry").path())
    {
        let Ok(source) = std::fs::read_to_string(dir.join("report.rmd")) else {
            continue;
        };
        let mut data = BTreeMap::new();
        folder_files(&dir.join("data"), "data/", &mut data);
        let name = dir.file_name().expect("name").to_string_lossy();
        reports.push((format!("examples/{name}"), source, data));
    }
    for page in std::fs::read_dir(root.join("docs/public/charts"))
        .expect("charts")
        .map(|e| e.expect("entry").path())
    {
        if page.extension().is_some_and(|e| e == "md") {
            let name = page.file_stem().expect("stem").to_string_lossy();
            let source = std::fs::read_to_string(&page).expect("page");
            reports.push((
                format!("docs/public/charts/{name}"),
                source,
                BTreeMap::new(),
            ));
        }
    }
    let mut charts = 0;
    for (label, source, data) in reports {
        for (i, json) in payloads(&source, data).iter().enumerate() {
            assert_frames_match(&format!("{label} chart {i}"), json);
            charts += 1;
        }
    }
    for name in ["us-cpi-components", "de-power-15min"] {
        for json in time_series(name, 4000) {
            assert_frames_match(name, &json);
            charts += 1;
        }
    }
    assert!(charts >= 150, "the corpus renders its charts ({charts})");
}
