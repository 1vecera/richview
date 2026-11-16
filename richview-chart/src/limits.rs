//! What a chart type draws and no more (FEA-CH-75). A limit is declared here, shown by `learn` and
//! the chart's page. Automatic point ceilings trigger disclosed representatives; explicit points: all bypasses
//! them. Other limits fail the chart with its count. No limit permits silent reduction.

/// One limit of one chart type: the most `what` it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    /// The chart type the limit belongs to.
    pub chart: &'static str,
    /// What is counted, in the plural ("links", "rows per panel").
    pub what: &'static str,
    /// The most there may be.
    pub max: usize,
}

const fn limit(chart: &'static str, what: &'static str, max: usize) -> Limit {
    Limit { chart, what, max }
}

/// Every declared limit, by chart type.
pub const LIMITS: &[Limit] = &[
    limit("line", "all-points observations", 10_000),
    limit("area", "all-points observations", 10_000),
    limit("scatter", "all-points observations", 10_000),
    limit("network", "nodes", 400),
    limit("network", "links", 2_000),
    limit("sankey", "nodes", 300),
    limit("sankey", "links", 2_000),
    limit("alluvial", "nodes", 300),
    limit("alluvial", "links", 2_000),
    limit("chord", "nodes", 300),
    limit("chord", "links", 2_000),
    limit("flow", "steps", 150),
    limit("flow", "connections", 600),
    limit("contour", "scattered rows per panel", 4_000),
    limit("wordcloud", "words per panel", 150),
    limit("packed-bubble", "bubbles per panel", 200),
    limit("vector", "arrows per panel", 1_500),
    limit("gantt", "tasks per panel", 60),
    limit("scatter3d", "points", 5_000),
    limit("parallel", "axes", 8),
    limit("splom", "dimensions", 6),
    limit("pivot", "rows", 40),
    limit("pivot", "columns", 12),
    limit("bar3d", "x categories", 24),
    limit("bar3d", "groups", 12),
    limit("surface", "scattered rows", 4_000),
    limit("surface", "grid nodes per axis", 100),
];

/// The most `what` the chart type `chart` takes.
///
/// Panics for an undeclared pair: a painter may only name a limit that is in [`LIMITS`], so the
/// table, `learn` and the page cannot drift from what is enforced.
pub fn max(chart: &str, what: &str) -> usize {
    LIMITS
        .iter()
        .find(|l| l.chart == chart && l.what == what)
        .unwrap_or_else(|| panic!("undeclared limit: {chart} {what}"))
        .max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_limit_is_declared_once_and_positive() {
        for (i, a) in LIMITS.iter().enumerate() {
            assert!(a.max > 0);
            assert!(
                LIMITS[i + 1..].iter().all(|b| (a.chart, a.what) != (b.chart, b.what)),
                "{a:?}"
            );
        }
    }
}
