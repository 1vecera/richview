//! Same-source full-render benchmark; fixtures are shared verbatim with the archived TS engine.
use serde_json::json;
use std::error::Error;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let fixtures = root.join("tmp/performance/fixtures");
    if std::env::args().any(|arg| arg == "--write-fixtures") {
        write_fixtures(&root, &fixtures)?;
    }
    if std::env::args().any(|arg| arg == "--fixtures-only") {
        return Ok(());
    }
    let iterations: usize = std::env::var("RICHVIEW_BENCH_ITERATIONS")
        .unwrap_or("100".into())
        .parse()?;
    if iterations == 0 {
        return Err("RICHVIEW_BENCH_ITERATIONS must be positive".into());
    }
    let mut results = vec![];
    for name in ["small", "six-types", "dense", "flagship"] {
        let file = fixtures.join(name).join("report.rmd");
        let source = fs::read_to_string(&file)?;
        // Resolve report-local style/media before measuring the pure renderer. Complete
        // offline runtime packaging is measured separately by the process-cold CLI harness.
        let (_, options) = richview_app::resolver::resolve_document_options_policy(
            &source,
            &file,
            &["scientific".into()],
            false,
            false,
        )?;
        for _ in 0..5 {
            black_box(
                richview_core::document::render(&source, &options).map_err(|e| format!("{e:?}"))?,
            );
        }
        let mut times = Vec::with_capacity(iterations);
        let mut bytes = 0;
        let mut chart_count = 0;
        for _ in 0..iterations {
            let start = Instant::now();
            let report = richview_core::document::render(black_box(&source), black_box(&options))
                .map_err(|e| format!("{e:?}"))?;
            if report.diagnostics.iter().any(|d| d.level == "error") {
                return Err(
                    format!("{name} contains source errors: {:?}", report.diagnostics).into(),
                );
            }
            times.push(start.elapsed().as_secs_f64() * 1000.0);
            bytes = report.html.len();
            chart_count = report.metadata["_richview"]["chart_count"]
                .as_u64()
                .unwrap_or(report.charts.len() as u64) as usize;
            black_box(report);
        }
        times.sort_by(f64::total_cmp);
        results.push(json!({"workload":name,"iterations":iterations,"source_bytes":source.len(),"html_bytes":bytes,"charts":chart_count,"median_ms":times[iterations/2],"p95_ms":times[((iterations as f64*0.95).ceil() as usize).saturating_sub(1)],"min_ms":times[0]}));
    }
    let memory = fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmHWM:"))
                .map(String::from)
        });
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"engine":"Rust","architecture":std::env::consts::ARCH,"os":std::env::consts::OS,"peak_rss":memory,"profile":"release opt-level=3 lto codegen-units=1","results":results})
        )?
    );
    Ok(())
}

fn write_fixtures(root: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(output)?;
    for name in ["small", "six-types", "dense", "flagship"] {
        fs::create_dir_all(output.join(name))?;
    }
    fs::copy(
        root.join("examples/sales-report/report.rmd"),
        output.join("small/report.rmd"),
    )?;
    let mut types = String::from("---\ntitle: Six chart contracts\nversion: 0.0.1\n---\n\n");
    for kind in ["line", "bar", "scatter", "area", "pie", "heatmap"] {
        let (data, encoding) = if kind == "heatmap" {
            (
                json!([{"x":"A","y":"One","value":10},{"x":"B","y":"One","value":18}]),
                json!({"x":"x","y":"y","color":"value"}),
            )
        } else if kind == "scatter" {
            (
                json!([{"x":1,"value":100,"weight":10},{"x":2,"value":140,"weight":25},{"x":3,"value":185,"weight":15}]),
                json!({"x":"x","y":"value","size":"weight"}),
            )
        } else {
            (
                json!([{"x":"Jan","value":100},{"x":"Feb","value":140},{"x":"Mar","value":185}]),
                json!({"x":"x","y":"value"}),
            )
        };
        let chart = json!({"type":kind,"title":format!("{kind} comparison"),"data":{"inline":data},"encoding":encoding});
        types.push_str(&format!(
            "```chart\n{}\n```\n\n",
            richview_core::yaml::to_string(&(&chart).into())
        ));
    }
    fs::write(output.join("six-types/report.rmd"), types)?;
    let observations = (0..35_040).map(|i| json!({"time":i,"value":((i as f64/50.0).sin()*20.0+100.0),"group":if i%2==0 {"A"} else {"B"}})).collect::<Vec<_>>();
    let chart = json!({"type":"line","title":"Dense series with exact observations","data":{"inline":observations},"encoding":{"x":"time","y":"value","color":"group"}});
    fs::write(
        output.join("dense/report.rmd"),
        format!(
            "---\ntitle: Dense series\nversion: 0.0.1\n---\n\n```chart\n{}\n```\n",
            richview_core::yaml::to_string(&(&chart).into())
        ),
    )?;
    let flagship_file = root.join("examples/openai-vs-anthropic/report.rmd");
    let source = fs::read_to_string(&flagship_file)?;
    fs::write(output.join("flagship/report.rmd"), source)?;
    let flagship_folder = flagship_file.parent().unwrap();
    for asset in [
        "report.rstyle.yaml",
        "README.md",
        "assets",
        "media",
        "data",
        "ledger",
    ] {
        copy_assets(
            &flagship_folder.join(asset),
            &output.join("flagship").join(asset),
        )?;
    }
    // Native HTTP callers receive the same host-resolved local references as the CLI.
    for name in ["small", "six-types", "dense", "flagship"] {
        let file = output.join(name).join("report.rmd");
        let source = fs::read_to_string(&file)?;
        let (_, options) = richview_app::resolver::resolve_document_options_policy(
            &source,
            &file,
            &["scientific".into()],
            false,
            false,
        )?;
        fs::write(
            output.join(name).join("render-options.json"),
            serde_json::to_vec(&options)?,
        )?;
    }
    Ok(())
}

fn copy_assets(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    if source.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_assets(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else {
        fs::copy(source, destination)?;
    }
    Ok(())
}
