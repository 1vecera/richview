// A three-type registry in the shape of crates/richview-core/src/registry.rs.
pub const CHART_TYPES: &[ChartType] = &[
    ChartType {
        name: "bar",
        summary: "Bars.",
    },
    ChartType {
        name: "line",
        summary: "A line.",
    },
    ChartType {
        name: "pie",
        summary: "Slices.",
    },
];
