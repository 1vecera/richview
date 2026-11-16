//! End-to-end check against a real MetricFlow runtime.
//!
//! This test is ignored by default and needs the `RICHVIEW_MF_OKURKA_DIR`,
//! `DBT_PROFILES_DIR` and `OKURKA_DBT_PATH` environment variables, so a host
//! without a wired-up Okurka installation skips it by name instead of passing
//! silently. Run with `--ignored`; without the variables it fails with the
//! reason. When they are set, the test issues a real
//! `mf query` against the Okurka synthetic warehouse through the connector
//! registry and verifies the chart carries typed rows plus a MetricFlow
//! provenance receipt.

use richview_app::connectors::Registry;
use richview_app::connectors::metricflow_adapter::{MetricFlowConnector, MetricFlowRuntime};
use richview_app::connectors::spec::ProvenanceSource;
use richview_app::resolver;
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::PathBuf};

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/metricflow-report")
}

#[test]
#[ignore = "needs the MetricFlow runtime (RICHVIEW_MF_OKURKA_DIR, DBT_PROFILES_DIR, OKURKA_DBT_PATH)"]
fn okurka_metricflow_runtime_returns_real_rows_through_the_connector() {
    let needed = |name: &str, what: &str| {
        std::env::var(name).unwrap_or_else(|_| {
            panic!("the MetricFlow runtime is not wired up: set {name} ({what})")
        })
    };
    let project_dir = needed("RICHVIEW_MF_OKURKA_DIR", "the Okurka dbt project directory");
    let profiles_dir = needed("DBT_PROFILES_DIR", "the dbt profiles directory");
    let warehouse = needed("OKURKA_DBT_PATH", "the Okurka duckdb warehouse");
    let target = std::env::var("OKURKA_DBT_TARGET").unwrap_or_else(|_| "serve".to_string());
    let compat_dir = std::env::var("RICHVIEW_MF_COMPAT_DIR")
        .unwrap_or_else(|_| format!("{}/../../tools/dbt-compat", project_dir));

    let root = example_root();
    let report = root.join("report.rmd");
    let source = fs::read_to_string(&report).unwrap();

    let cache_root = root.join(".richview-cache");
    let _ = fs::remove_dir_all(&cache_root);

    let runtime = MetricFlowRuntime {
        launcher: vec![
            "uv".into(),
            "--project".into(),
            compat_dir,
            "run".into(),
            "--frozen".into(),
            "--".into(),
            "mf".into(),
        ],
        project_dir: PathBuf::from(&project_dir),
        env: BTreeMap::from([
            ("DBT_PROFILES_DIR".into(), profiles_dir),
            ("OKURKA_DBT_PATH".into(), warehouse),
            ("OKURKA_DBT_TARGET".into(), target),
        ]),
    };
    let mut registry = Registry::new(cache_root);
    registry.register("okurka", MetricFlowConnector::new(runtime));

    let (document, _options) = resolver::resolve_document_options_full(
        &source,
        &report,
        &[],
        false,
        false,
        Some(&registry),
    )
    .expect("resolve");

    let chart = document.blocks.iter().find(|b| b.tag == "chart").unwrap();
    let rows = chart
        .value
        .pointer("/data/inline")
        .and_then(Value::as_array)
        .expect("inline rows");
    assert!(!rows.is_empty(), "mf returned zero rows");
    for row in rows {
        assert!(
            row.get("okurka_units").is_some(),
            "row missing metric column"
        );
        assert!(
            row.get("metric_time__month").is_some(),
            "row missing group_by column"
        );
    }
    let provenance = chart
        .value
        .pointer("/data/_provenance")
        .expect("provenance recorded");
    let kind: ProvenanceSource =
        serde_json::from_value(provenance.get("source").unwrap().clone()).unwrap();
    assert_eq!(kind, ProvenanceSource::Metricflow);
    assert!(
        provenance
            .get("fingerprint")
            .and_then(Value::as_str)
            .map(|f| f.len() == 64)
            .unwrap_or(false)
    );
}
