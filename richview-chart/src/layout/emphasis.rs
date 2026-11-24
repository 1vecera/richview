//! An author's highlight as marks (TYP-78). The rows `highlight.include` matches keep their
//! colour, their names and their values; with `highlight.mute` on, every other mark takes the
//! style's quiet ink (`ink_faint`, held to 3:1 on every surface by STY-25) and loses the names and
//! values the chart would add by itself. Line and area mute whole series, bars, lollipops and
//! scatter points mute row by row. Muted marks paint first, so a highlighted mark is never under one.

use super::Ctx;
use crate::block::ChartType;
use crate::diag::Level;
use crate::rows;
use serde_json::json;

/// Which rows and series an author's highlight leaves in colour.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Emphasis {
    /// Per source row: whether `highlight.include` matches it.
    rows: Vec<bool>,
    /// Per series: whether any of its drawn rows matches.
    series: Vec<bool>,
    /// Line and area recede by series, the other types by row.
    by_series: bool,
}

impl Emphasis {
    /// Whether `highlight.mute` is on: by default on line, area, bar and lollipop charts and off on
    /// a scatter, whose highlights are named points among coloured ones. Other types never mute.
    pub(crate) fn mutes(ctx: &Ctx) -> bool {
        let block = ctx.block;
        let Some(h) = &block.highlight else { return false };
        let kind = block.kind;
        if kind == ChartType::Scatter {
            h.mute.unwrap_or(false)
        } else if matches!(kind, ChartType::Line | ChartType::Area | ChartType::Bar) || kind.name() == "lollipop" {
            h.mute.unwrap_or(true)
        } else {
            false
        }
    }

    /// Resolve the block's highlight against `members`, each a row the chart draws and its series.
    /// `None` when the block has no highlight, mute is off, or `include` matches no drawn row: a
    /// highlight that finds nothing mutes nothing. Every type but scatter warns here for each
    /// `include` value no row matches; a scatter's point names warn for it.
    pub(crate) fn resolve(ctx: &mut Ctx, members: &[(usize, usize)]) -> Option<Self> {
        let block = ctx.block;
        let h = block.highlight.as_ref()?;
        if block.kind != ChartType::Scatter {
            for v in &h.include {
                if !ctx.rows.iter().any(|r| matches(r, &h.by, v)) {
                    ctx.check(
                        "unresolved-highlight",
                        Level::Warning,
                        format!("Highlight '{v}' matches no row"),
                        Some("Check the spelling against the data."),
                        Some("chart.highlight.include".into()),
                        None,
                        json!({"value": v}),
                    );
                }
            }
        }
        if !Self::mutes(ctx) {
            return None;
        }
        let mut emphasis = Emphasis {
            rows: vec![false; ctx.rows.len()],
            series: vec![false; ctx.series.len()],
            by_series: matches!(block.kind, ChartType::Line | ChartType::Area),
        };
        let mut any = false;
        for &(row, series) in members {
            if ctx
                .rows
                .get(row)
                .is_some_and(|r| h.include.iter().any(|v| matches(r, &h.by, v)))
            {
                emphasis.rows[row] = true;
                if let Some(s) = emphasis.series.get_mut(series) {
                    *s = true;
                }
                any = true;
            }
        }
        any.then_some(emphasis)
    }

    /// Whether series `s` recedes as a whole: none of its rows is highlighted. Its legend entry,
    /// line-end and inside names and its automatic values go with it.
    pub(crate) fn mutes_series(&self, s: usize) -> bool {
        !self.series.get(s).copied().unwrap_or(true)
    }

    /// Whether the mark of `row` in series `s` recedes: its series on a line or area chart, the
    /// row itself on the other types.
    pub(crate) fn mutes_mark(&self, row: usize, s: usize) -> bool {
        if self.by_series {
            self.mutes_series(s)
        } else {
            !self.rows.get(row).copied().unwrap_or(true)
        }
    }
}

impl Emphasis {
    /// The highlighted series, in series order: ink an authored note keeps off, where a muted
    /// series is background it may cross.
    #[allow(dead_code)] // read by `place::annotations` on the annotations branch
    pub(crate) fn series(&self) -> Vec<usize> {
        (0..self.series.len()).filter(|&s| self.series[s]).collect()
    }

    /// The indices into `observations`, a panel's list in the order its painter made it, of the
    /// observations whose mark is highlighted.
    #[allow(dead_code)] // read by `place::annotations` on the annotations branch
    pub(crate) fn observations(&self, observations: &[crate::geometry::Observation]) -> Vec<usize> {
        observations
            .iter()
            .enumerate()
            .filter(|(_, o)| match (o.row, o.series) {
                (Some(row), Some(s)) => !self.mutes_mark(row, s),
                (_, Some(s)) => !self.mutes_series(s),
                _ => false,
            })
            .map(|(k, _)| k)
            .collect()
    }
}

/// Whether row `r`'s `by` value is `v`, as the highlight compares them: text, ignoring ASCII case.
pub(crate) fn matches(r: &serde_json::Value, by: &str, v: &str) -> bool {
    rows::text(r, by).is_some_and(|t| t.eq_ignore_ascii_case(v))
}
