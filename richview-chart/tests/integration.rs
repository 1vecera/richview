//! The crate's integration tests as one test binary: each `tests/<name>.rs` is a module here, so
//! the crate links one executable instead of one for every file. Run one file's tests with
//! `cargo test -p richview-chart --test integration -- <name>::`.

mod common;

#[path = "alluvial_contract.rs"]
mod alluvial_contract;
#[path = "area_contract.rs"]
mod area_contract;
#[path = "axis_domains.rs"]
mod axis_domains;
#[path = "band_contract.rs"]
mod band_contract;
#[path = "bar3d_contract.rs"]
mod bar3d_contract;
#[path = "bar_contract.rs"]
mod bar_contract;
#[path = "bin2d_contract.rs"]
mod bin2d_contract;
#[path = "box_summary_contract.rs"]
mod box_summary_contract;
#[path = "boxplot_contract.rs"]
mod boxplot_contract;
#[path = "bullet_contract.rs"]
mod bullet_contract;
#[path = "calendar_contract.rs"]
mod calendar_contract;
#[path = "candlestick_contract.rs"]
mod candlestick_contract;
#[path = "chart_chrome.rs"]
mod chart_chrome;
#[path = "chord_contract.rs"]
mod chord_contract;
#[path = "combo_contract.rs"]
mod combo_contract;
#[path = "contour_contract.rs"]
mod contour_contract;
#[path = "controls_integration.rs"]
mod controls_integration;
#[path = "count_agreement.rs"]
mod count_agreement;
#[path = "data_integrity_contracts.rs"]
mod data_integrity_contracts;
#[path = "decimal_numbers.rs"]
mod decimal_numbers;
#[path = "density_contract.rs"]
mod density_contract;
#[path = "derived_precision.rs"]
mod derived_precision;
#[path = "derived_replay.rs"]
mod derived_replay;
#[path = "donut_contract.rs"]
mod donut_contract;
#[path = "dotplot_contract.rs"]
mod dotplot_contract;
#[path = "dumbbell_contract.rs"]
mod dumbbell_contract;
#[path = "ecdf_contract.rs"]
mod ecdf_contract;
#[path = "every_type.rs"]
mod every_type;
#[path = "flow_contract.rs"]
mod flow_contract;
#[path = "funnel_contract.rs"]
mod funnel_contract;
#[path = "gantt_contract.rs"]
mod gantt_contract;
#[path = "gauge_contract.rs"]
mod gauge_contract;
#[path = "geometry_contracts.rs"]
mod geometry_contracts;
#[path = "geometry_gallery.rs"]
mod geometry_gallery;
#[path = "geometry_rules.rs"]
mod geometry_rules;
#[path = "heatmap_contract.rs"]
mod heatmap_contract;
#[path = "hexbin_contract.rs"]
mod hexbin_contract;
#[path = "highlight_mute.rs"]
mod highlight_mute;
#[path = "histogram_contract.rs"]
mod histogram_contract;
#[path = "icicle_contract.rs"]
mod icicle_contract;
#[path = "interval_contract.rs"]
mod interval_contract;
#[path = "layout_reuse.rs"]
mod layout_reuse;
#[path = "legend_cues.rs"]
mod legend_cues;
#[path = "line_contract.rs"]
mod line_contract;
#[path = "lollipop_contract.rs"]
mod lollipop_contract;
#[path = "map_contract.rs"]
mod map_contract;
#[path = "mark_observations.rs"]
mod mark_observations;
#[path = "network_contract.rs"]
mod network_contract;
#[path = "numeric_safety_contract.rs"]
mod numeric_safety_contract;
#[path = "numerical_ranges.rs"]
mod numerical_ranges;
#[path = "ohlc_contract.rs"]
mod ohlc_contract;
#[path = "packed_bubble_contract.rs"]
mod packed_bubble_contract;
#[path = "parallel_contract.rs"]
mod parallel_contract;
#[path = "pareto_contract.rs"]
mod pareto_contract;
#[path = "pie_contract.rs"]
mod pie_contract;
#[path = "pie_labels.rs"]
mod pie_labels;
#[path = "pivot_contract.rs"]
mod pivot_contract;
#[path = "point_modes.rs"]
mod point_modes;
#[path = "prepared_layout.rs"]
mod prepared_layout;
#[path = "qq_contract.rs"]
mod qq_contract;
#[path = "radar_contract.rs"]
mod radar_contract;
#[path = "readout_anchor.rs"]
mod readout_anchor;
#[path = "ribbon_contract.rs"]
mod ribbon_contract;
#[path = "rose_contract.rs"]
mod rose_contract;
#[path = "rotate3d_contract.rs"]
mod rotate3d_contract;
#[path = "sankey_contract.rs"]
mod sankey_contract;
#[path = "scatter3d_contract.rs"]
mod scatter3d_contract;
#[path = "scatter_contract.rs"]
mod scatter_contract;
#[path = "sparse_values.rs"]
mod sparse_values;
#[path = "splom_contract.rs"]
mod splom_contract;
#[path = "stack_contract.rs"]
mod stack_contract;
#[path = "streamgraph_contract.rs"]
mod streamgraph_contract;
#[path = "strip_contract.rs"]
mod strip_contract;
#[path = "sunburst_contract.rs"]
mod sunburst_contract;
#[path = "surface_contract.rs"]
mod surface_contract;
#[path = "svg_export.rs"]
mod svg_export;
#[path = "temporal_value_forms.rs"]
mod temporal_value_forms;
#[path = "tree_contract.rs"]
mod tree_contract;
#[path = "treemap_contract.rs"]
mod treemap_contract;
#[path = "value_digits.rs"]
mod value_digits;
#[path = "value_labels.rs"]
mod value_labels;
#[path = "vector_contract.rs"]
mod vector_contract;
#[path = "venn_contract.rs"]
mod venn_contract;
#[path = "violin_contract.rs"]
mod violin_contract;
#[path = "waffle_contract.rs"]
mod waffle_contract;
#[path = "waterfall_contract.rs"]
mod waterfall_contract;
#[path = "wordcloud_contract.rs"]
mod wordcloud_contract;

/// With `autotests = false` a `tests/<name>.rs` that is not declared above is no target and never runs.
#[test]
fn every_test_file_is_a_module_of_this_binary() {
    let root = include_str!("integration.rs");
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(folder).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if name.ends_with(".rs") && name != "integration.rs" && !root.contains(&format!("#[path = \"{name}\"]")) {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        "declare these in tests/integration.rs, or they never run: {missing:?}"
    );
}
