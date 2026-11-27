//! The crate's integration tests as one test binary: each `tests/<name>.rs` is a module here, so
//! the crate links one executable instead of one for every file. Run one file's tests with
//! `cargo test -p richview-core --test integration -- <name>::`.

mod common;

#[path = "asset_failures.rs"]
mod asset_failures;
#[path = "block_fields.rs"]
mod block_fields;
#[path = "blocks.rs"]
mod blocks;
#[path = "boxplot_contract.rs"]
mod boxplot_contract;
#[path = "chart_types.rs"]
mod chart_types;
#[path = "collapsible_sections.rs"]
mod collapsible_sections;
#[path = "column_order.rs"]
mod column_order;
#[path = "columns.rs"]
mod columns;
#[path = "contracts.rs"]
mod contracts;
#[path = "csv_decimal_source.rs"]
mod csv_decimal_source;
#[path = "data_typing.rs"]
mod data_typing;
#[path = "document_api.rs"]
mod document_api;
#[path = "expand_and_code.rs"]
mod expand_and_code;
#[path = "figures.rs"]
mod figures;
#[path = "flow_chart.rs"]
mod flow_chart;
#[path = "fonts.rs"]
mod fonts;
#[path = "geometry_invariants.rs"]
mod geometry_invariants;
#[path = "histogram_contract.rs"]
mod histogram_contract;
#[path = "hostile.rs"]
mod hostile;
#[path = "hostile_findings.rs"]
mod hostile_findings;
#[path = "identity.rs"]
mod identity;
#[path = "interval_contract.rs"]
mod interval_contract;
#[path = "legend_cues.rs"]
mod legend_cues;
#[path = "markdown_ext.rs"]
mod markdown_ext;
#[path = "markdown_scans.rs"]
mod markdown_scans;
#[path = "numeric_range_contract.rs"]
mod numeric_range_contract;
#[path = "palettes.rs"]
mod palettes;
#[path = "panel_anchors.rs"]
mod panel_anchors;
#[path = "point_modes.rs"]
mod point_modes;
#[path = "print_a11y.rs"]
mod print_a11y;
#[path = "provenance.rs"]
mod provenance;
#[path = "registry.rs"]
mod registry;
#[path = "report_checks.rs"]
mod report_checks;
#[path = "shell.rs"]
mod shell;
#[path = "slides.rs"]
mod slides;
#[path = "spec_docs.rs"]
mod spec_docs;
#[path = "style_backgrounds.rs"]
mod style_backgrounds;
#[path = "style_checks.rs"]
mod style_checks;
#[path = "style_presets.rs"]
mod style_presets;
#[path = "style_rules.rs"]
mod style_rules;
#[path = "styles.rs"]
mod styles;
#[path = "table_tools.rs"]
mod table_tools;
#[path = "temporal_decimal_source.rs"]
mod temporal_decimal_source;
#[path = "type_contracts.rs"]
mod type_contracts;
#[path = "yaml_profile.rs"]
mod yaml_profile;

/// With `autotests = false` a `tests/<name>.rs` that is not declared above is no target and never runs.
#[test]
fn every_test_file_is_a_module_of_this_binary() {
    let root = include_str!("integration.rs");
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(folder).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if name.ends_with(".rs")
            && name != "integration.rs"
            && !root.contains(&format!("#[path = \"{name}\"]"))
        {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        "declare these in tests/integration.rs, or they never run: {missing:?}"
    );
}
