//! The chart payload a report embeds for each figure (ARC-06): the validated chart block, its
//! resolved rows at the author's precision (TYP-36), the style's chart theme and the report's
//! render options. Core writes it; the report runtime reads it and lays the chart out at the
//! width the figure occupies.

use crate::layout::ChartTheme;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// The base number of position intervals (TYP-36): more where shown precision distinguishes
/// positions, fewer when an integer scale fits its native unit resolution.
pub const QUANT_STEPS: u32 = 4095;

/// A numeric column written as position indices `k`, decoded with `min + k * (max - min) / steps`.
/// The grid records a native resolution only when its positions preserve that data resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Quant {
    pub min: f64,
    pub max: f64,
    pub steps: u32,
    /// The data's own resolution when every value fits exactly, rather than a lossy grid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<f64>,
}

impl Quant {
    /// One scale's grid from its exact source values and the values the author shows. Core and
    /// native layouts use the same extent and precision rule, including when columns share a scale.
    pub fn for_values(exact: impl IntoIterator<Item = f64>, shown: impl IntoIterator<Item = f64>) -> Option<Quant> {
        let mut integers = true;
        let extent = Self::over(
            exact
                .into_iter()
                .filter(|v| v.is_finite())
                .inspect(|v| integers &= v.fract() == 0.0),
        )?;
        let grid = extent.on_grid(QUANT_STEPS, integers.then_some(1.0))?;
        let places = shown
            .into_iter()
            .filter(|v| *v != 0.0)
            .map(|value| {
                let text = format!("{value:e}");
                let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
                let fraction = mantissa.split_once('.').map_or(0, |(_, f)| f.len() as i32);
                fraction - exponent.parse::<i32>().unwrap_or(0)
            })
            .max()
            .unwrap_or(0);
        let resolution = 10f64.powi(-places);
        let steps = ((grid.max - grid.min) / resolution).round();
        Some(
            if places <= 15 && steps > f64::from(grid.steps) && steps <= f64::from(u32::MAX >> 1) {
                Quant {
                    steps: steps as u32,
                    ..grid
                }
            } else {
                grid
            },
        )
    }

    /// The column's grid over `values`, or `None` when it has no finite value.
    pub fn over(values: impl IntoIterator<Item = f64>) -> Option<Quant> {
        let mut range: Option<(f64, f64)> = None;
        for v in values.into_iter().filter(|v| v.is_finite()) {
            range = Some(match range {
                Some((lo, hi)) => (lo.min(v), hi.max(v)),
                None => (v, v),
            });
        }
        range.map(|(min, max)| Quant {
            min,
            max,
            steps: QUANT_STEPS,
            resolution: None,
        })
    }

    /// The grid over the same values with its ends moved outward to one digit finer than the
    /// span's leading digit, so the ends say little more than an axis label does, and with at
    /// most `steps` steps, a whole number of them in each of those digits: a range that spans
    /// zero has a step exactly at zero, and a narrow range keeps its steps. A span with no usable
    /// scale (zero, subnormal, or a power of ten that overflows) has no grid; its values travel
    /// at the author's precision. When every value uses a known `resolution`, a smaller range
    /// uses that resolution instead of inventing finer positions (dates and integers, TYP-36).
    pub fn on_grid(&self, steps: u32, resolution: Option<f64>) -> Option<Quant> {
        let span = self.max - self.min;
        if !(span.is_finite() && span > 0.0) || !span.is_normal() || steps == 0 {
            return None;
        }
        if !(1e-290..1e290).contains(&span) || self.min.abs().max(self.max.abs()) > 1e290 {
            return None;
        }
        if let Some(resolution) = resolution.filter(|r| r.is_finite() && *r > 0.0) {
            let units = span / resolution;
            if units >= 1.0 && units <= f64::from(steps) && units.fract() == 0.0 {
                return Some(Quant {
                    min: self.min,
                    max: self.max,
                    steps: units as u32,
                    resolution: Some(resolution),
                });
            }
        }
        let e = span.log10().floor() as i32 - 1;
        let scale = 10f64.powi(-e);
        // The ends as whole numbers of units, rounded outward; a decimal built from an integer and
        // a power of ten is the nearest double to it, so the ends are clean (100.6, not
        // 100.60000000000001).
        let lo = (self.min * scale).floor();
        let hi = (self.max * scale).ceil();
        if !lo.is_finite() || !hi.is_finite() || hi <= lo || hi - lo > 1e6 {
            return None;
        }
        let units = (hi - lo) as u64;
        let per_unit = (u64::from(steps) / units).max(1);
        let unit = |k: f64| if e < 0 { k / scale } else { k * 10f64.powi(e) };
        let grid = Quant {
            min: unit(lo),
            max: unit(hi),
            steps: u32::try_from(units * per_unit).ok()?,
            resolution: None,
        };
        (grid.min.is_finite() && grid.max.is_finite() && grid.max > grid.min).then_some(grid)
    }

    /// The grid step `value` is nearest to, counted from `min`. A grid that spans zero has a step
    /// at zero, and a value keeps its sign: a small positive value takes the first step above
    /// zero and a small negative one the first below (never a bar of nothing or a negative one).
    pub fn index(&self, value: f64) -> u64 {
        let span = self.max - self.min;
        if span <= 0.0 || self.steps == 0 {
            return 0;
        }
        let n = f64::from(self.steps);
        let mut k = ((value - self.min) / span * n).round().clamp(0.0, n);
        if self.min <= 0.0 && self.max >= 0.0 {
            let zero = (-self.min / span * n).round();
            if value > 0.0 && k <= zero {
                k = (zero + 1.0).min(n);
            } else if value < 0.0 && k >= zero {
                k = (zero - 1.0).max(0.0);
            }
        }
        k as u64
    }

    /// The value at grid step `k`.
    pub fn at(&self, k: u64) -> f64 {
        let span = self.max - self.min;
        if span <= 0.0 || self.steps == 0 {
            return self.min;
        }
        self.exact_zero(self.min + k as f64 * span / f64::from(self.steps))
    }

    /// A value within a billionth of a step of zero is zero: a grid with a step at zero gives
    /// exactly zero there, whatever the float sum says.
    fn exact_zero(&self, value: f64) -> f64 {
        let step = (self.max - self.min) / f64::from(self.steps.max(1));
        let float_error = self.min.abs().max(self.max.abs()) * 1e-14;
        if value.abs() < (step * 1e-9).max(float_error).min(step / 2.0) {
            0.0
        } else {
            value
        }
    }

    /// The grid step for `value`, which readers see under `format`. A grid refined past
    /// [`QUANT_STEPS`] steps, because some values of its scale show finer digits, would place a
    /// value shown coarser at a finer digit than it shows (`10,001.2` on a grid of units, behind a
    /// readout of `10.0 thousand`). Such a value takes its shown value when that lies within half a
    /// base step of it, and otherwise the nearest of the base grid's steps, or that step's shown
    /// value when it lies as near: no position says more than the author shows or the default view
    /// draws (TYP-36, FEA-CH-37). A value at a step it returns keeps that step.
    pub fn index_shown(&self, value: f64, format: &crate::numbers::Format) -> u64 {
        if self.steps <= QUANT_STEPS {
            return self.index(value);
        }
        let half = (self.max - self.min) / f64::from(QUANT_STEPS) / 2.0;
        let near_shown = |v: f64| {
            let shown = crate::numbers::shown(v, format);
            ((shown - v).abs() <= half).then_some(shown)
        };
        let base = Quant {
            steps: QUANT_STEPS,
            resolution: None,
            ..self.clone()
        };
        let on_base = self.snap(base.snap(value));
        self.index(near_shown(value).or_else(|| near_shown(on_base)).unwrap_or(on_base))
    }

    /// `value` moved to the nearest grid step, keeping its sign and an exact zero.
    pub fn snap(&self, value: f64) -> f64 {
        if value == 0.0 && self.min <= 0.0 && self.max >= 0.0 {
            return 0.0;
        }
        if (self.max - self.min) <= 0.0 || self.steps == 0 {
            return self.min;
        }
        self.at(self.index(value))
    }
}

/// One named value of a readout, formatted at the author's precision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TextLine {
    pub name: String,
    pub value: String,
    pub derived: bool,
    pub series: Option<usize>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub names_series: bool,
}

/// The structures the exact-row layout derived in one control state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StateDerived {
    /// The control state; empty when the chart has no controls.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub state: Vec<usize>,
    pub derived: Vec<crate::derived::Derived>,
}

/// The readout of one observation as the exact-row layout produced it (TYP-36), so the report
/// runtime shows it instead of formatting a quantized number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ObservationText {
    /// The control state this text belongs to; empty when the chart has no controls.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub state: Vec<usize>,
    pub panel: usize,
    pub obs: usize,
    /// The observation's stable mark id (`Geometry::mark_ids`), which stays the same when a
    /// legend toggle or a zoom drops other observations; `obs` only lines up while none are.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub id: String,
    pub heading: String,
    pub lines: Vec<TextLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Payload {
    /// Figure id, unique in the report.
    pub id: String,
    /// The chart block as validated by core.
    pub block: Value,
    /// Column names in source order (TYP-25).
    pub columns: Vec<String>,
    /// Rows as arrays in `columns` order. A `quant` column holds each value's grid step as a
    /// whole number; other numbers are rounded to readout precision.
    pub rows: Vec<Vec<Value>>,
    /// The grid of each numeric channel column (TYP-36).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub quant: BTreeMap<String, Quant>,
    /// For each `quant` column, its numbers at the author's readout precision, row by row, for
    /// the Table and the CSV; never more digits than the author chose (FEA-CH-37).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub shown: BTreeMap<String, Vec<Value>>,
    /// The on-chart value texts (data labels, annotations, key values) of the exact-row layout in
    /// paint order, which the runtime shows in place of any it would print from the grid.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<crate::geometry::MarkText>,
    /// What the exact-row layout derived (bins, quartiles, orderings, flows), which the runtime's
    /// layout from grid rows draws instead of deriving its own (TYP-36).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub derived: Vec<StateDerived>,
    /// Readouts laid out from the exact rows, by panel and observation.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub text: Vec<ObservationText>,
    pub theme: ChartTheme,
    /// The light counterpart's theme for browser printing, when the style has one (FEA-CH-67).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub print: Option<ChartTheme>,
    /// Report language (TYP-08).
    pub lang: String,
    /// Report-level entity colours (FEA-CH-63).
    pub colors: Vec<(String, usize)>,
    /// Base name for downloads, from the chart title.
    pub file: String,
}

impl Payload {
    /// The rows with each `quant` column's numbers at the author's readout precision, as the
    /// Table and the CSV show them.
    pub fn shown_rows(&self) -> Vec<Vec<Value>> {
        let mut rows = self.rows.clone();
        for (name, values) in &self.shown {
            let Some(at) = self.columns.iter().position(|c| c == name) else {
                continue;
            };
            for (row, value) in rows.iter_mut().zip(values) {
                if let Some(cell) = row.get_mut(at) {
                    *cell = value.clone();
                }
            }
        }
        rows
    }

    /// Rows as objects, the engine's input.
    pub fn objects(&self) -> Vec<Value> {
        let grids: Vec<Option<&Quant>> = self.columns.iter().map(|c| self.quant.get(c)).collect();
        self.rows
            .iter()
            .map(|row| {
                let mut map = serde_json::Map::new();
                for ((k, v), grid) in self.columns.iter().zip(row).zip(&grids) {
                    let value = match (grid, v.as_u64()) {
                        (Some(grid), Some(step)) => serde_json::json!(grid.at(step)),
                        _ => v.clone(),
                    };
                    map.insert(k.clone(), value);
                }
                Value::Object(map)
            })
            .collect()
    }
}

/// A file name from a chart title: `support-load-fell-in-every-region` (FEA-CH-48).
pub fn file_name(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let trimmed = out.trim_end_matches('-').chars().take(80).collect::<String>();
    if trimmed.is_empty() { "chart".into() } else { trimmed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grid_covers_its_values_keeps_short_decimals_and_has_a_step_at_zero() {
        for (lo, hi, steps) in [
            (-0.0733, 1.0, QUANT_STEPS),
            (100.016, 101.69, QUANT_STEPS),
            (0.03, 8.0, QUANT_STEPS),
            (-5.0, 5.0, QUANT_STEPS),
            (-3.3, 91.7, QUANT_STEPS),
            (12.0, 99.0, QUANT_STEPS),
            (-1e-9, 3e-9, QUANT_STEPS),
            (1.0, 2.0, QUANT_STEPS),
        ] {
            let grid = Quant {
                min: lo,
                max: hi,
                steps: QUANT_STEPS,
                resolution: None,
            }
            .on_grid(steps, None)
            .expect("a usable span has a grid");
            assert!(grid.min <= lo && grid.max >= hi, "{lo}..{hi} -> {grid:?}");
            assert!(
                grid.steps <= steps && grid.steps + 100 >= steps,
                "{lo}..{hi} steps {}",
                grid.steps
            );
            // The ends are short decimals: one digit finer than the span.
            assert!(
                format!("{:e}", grid.min).len() <= 12 && format!("{:e}", grid.max).len() <= 12,
                "{grid:?}"
            );
            if lo < 0.0 && hi > 0.0 {
                let k = grid.index(0.0);
                assert_eq!(grid.at(k), 0.0, "{lo}..{hi}: the step at zero -> {grid:?}");
                assert_eq!(grid.snap(0.0), 0.0);
                let tiny = (hi * 1e-4).min(-lo * 1e-4);
                assert!(grid.snap(tiny) > 0.0, "{lo}..{hi}: a small positive stays positive");
                assert!(grid.snap(-tiny) < 0.0, "{lo}..{hi}: a small negative stays negative");
            }
        }
    }

    #[test]
    fn a_refined_grid_places_a_value_no_finer_than_it_shows() {
        use crate::numbers::{Format, shown};
        let format = Format::default();
        // Income from 650 to 65,500: whole numbers under 10,000 show their units (FEA-CH-22), so
        // the grid refines to 66,000 steps of one; from 10,000 a value shows three digits.
        let grid = Quant::for_values([650.0, 9501.17, 65_500.0], [650.0, 9501.0, 65_500.0]).expect("a grid");
        assert_eq!((grid.min, grid.max, grid.steps), (0.0, 66_000.0, 66_000));
        let base = Quant {
            steps: QUANT_STEPS,
            ..grid.clone()
        };
        let at = |v: f64| grid.at(grid.index_shown(v, &format));
        // A value showing its units keeps them; one shown as 10.0 thousand takes 10,000, not
        // 10,001, and one too far from its shown value takes the base grid's step.
        assert_eq!(at(9501.17), 9501.0);
        assert_eq!(at(10_001.234_6), 10_000.0);
        assert_eq!(shown(10_045.67, &format), 10_000.0);
        assert_eq!(at(10_045.67), base.snap(10_045.67).round());
        assert_ne!(at(10_045.67), 10_046.0);
        for v in [0.0, 650.08, 9999.4, 9999.6, 10_008.2, 10_009.0, 33_333.3, 65_501.2] {
            let p = at(v);
            assert!((p - v).abs() <= 66_000.0 / f64::from(QUANT_STEPS), "{v} -> {p}");
            assert_eq!(at(p), p, "{v}: a value at its step keeps it");
            assert!(
                p == shown(p, &format) || (p - base.snap(p)).abs() <= 0.5,
                "{v} -> {p} says more than its shown value or the base grid"
            );
        }
        // Positions keep the order of their values.
        let ordered: Vec<f64> = (0..20_000).map(|k| 9_990.0 + f64::from(k) * 0.003).map(at).collect();
        assert!(ordered.windows(2).all(|w| w[0] <= w[1]));
        // A grid at the base step count or coarser places every value at its nearest step.
        let plain = Quant::for_values([0.0, 1000.0], [0.0, 1000.0]).expect("a grid");
        assert!(plain.steps <= QUANT_STEPS);
        assert_eq!(plain.index_shown(412.345, &format), plain.index(412.345));
    }

    #[test]
    fn file_names_follow_the_title() {
        assert_eq!(
            file_name("Support load fell in every region"),
            "support-load-fell-in-every-region"
        );
        assert_eq!(file_name("CO₂: 2024–2025!"), "co₂-2024-2025");
        assert_eq!(file_name("!!!"), "chart");
    }
}
