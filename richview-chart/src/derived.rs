//! Structures a layout derives from exact values (TYP-36). A painter computes the part of its
//! work that jumps with the data (bins, quartiles, orderings, flows, placements) through
//! [`Ctx::derive`](crate::layout::Ctx), which records it in the geometry. A layout from grid rows
//! that is handed those records draws the same marks as the layout from exact rows, so the browser
//! never recomputes what `check` and the PDF verified.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Significant digits of pure layout coordinates. Data-scale records use their own codecs.
const DIGITS: i32 = 6;

/// One derived structure of one panel, in data space (independent of the figure's width).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Derived {
    pub panel: usize,
    pub key: String,
    pub value: Value,
}

/// A statistic's position, or its shown value when no usable scale exists.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Position {
    Grid(u64),
    Shown(f64),
}

impl Position {
    pub(crate) fn encode(value: f64, grid: Option<&crate::payload::Quant>, format: &crate::numbers::Format) -> Self {
        match grid {
            Some(grid) => Self::Grid(grid.index_shown(value, format)),
            None => Self::Shown(crate::numbers::shown(value, format)),
        }
    }

    pub(crate) fn at(self, grid: Option<&crate::payload::Quant>) -> f64 {
        match self {
            Self::Grid(index) => grid.expect("a grid-index record carries its grid").at(index),
            Self::Shown(value) => value,
        }
    }
}

/// Bounds and positions on a source channel's grid, with shown-only values when no grid exists.
#[derive(Serialize, Deserialize)]
pub(crate) struct DataRange {
    pub grid: Option<crate::payload::Quant>,
    pub extent: [Position; 2],
}

impl DataRange {
    pub(crate) fn encode(
        extent: [f64; 2],
        grid: Option<crate::payload::Quant>,
        format: &crate::numbers::Format,
    ) -> Self {
        Self {
            extent: extent.map(|v| Position::encode(v, grid.as_ref(), format)),
            grid,
        }
    }

    pub(crate) fn bounds(&self) -> [f64; 2] {
        self.extent.map(|p| p.at(self.grid.as_ref()))
    }

    pub(crate) fn project(&self, value: f64, format: &crate::numbers::Format) -> f64 {
        self.grid.as_ref().map_or_else(
            || crate::numbers::shown(value, format),
            |grid| grid.at(grid.index_shown(value, format)),
        )
    }
}

/// Values of one source channel on its grid, or as shown values when it has none: a record's
/// coordinates and values in the precision the payload rows already allow (FEA-CH-37).
#[derive(Serialize, Deserialize)]
pub(crate) struct Positions {
    pub grid: Option<crate::payload::Quant>,
    pub values: Vec<Position>,
}

impl Positions {
    pub(crate) fn encode(
        values: impl IntoIterator<Item = f64>,
        grid: Option<crate::payload::Quant>,
        format: &crate::numbers::Format,
    ) -> Self {
        let values = values
            .into_iter()
            .map(|v| Position::encode(v, grid.as_ref(), format))
            .collect();
        Self { grid, values }
    }

    pub(crate) fn decode(&self) -> Vec<f64> {
        self.values.iter().map(|p| p.at(self.grid.as_ref())).collect()
    }
}

/// Refine a record's statistic grid without changing its source observation grid or bounds.
pub(crate) fn statistic_grid(grid: Option<crate::payload::Quant>) -> Option<crate::payload::Quant> {
    grid.map(|mut grid| {
        if grid.steps < crate::payload::QUANT_STEPS {
            grid = crate::payload::Quant::over([grid.min, grid.max]).expect("the supplied grid has finite bounds");
        }
        grid.resolution = None;
        grid
    })
}

/// Keep the precision of shown statistics on a wide scale as well as its baseline/finer grid.
pub(crate) fn statistic_grid_with_shown(
    grid: Option<crate::payload::Quant>,
    shown: impl IntoIterator<Item = f64>,
) -> Option<crate::payload::Quant> {
    let mut grid = statistic_grid(grid)?;
    if let Some(chosen) = crate::payload::Quant::for_values([grid.min, grid.max], shown) {
        let steps = ((grid.max - grid.min) / (chosen.max - chosen.min) * f64::from(chosen.steps)).round();
        if steps.is_finite() && steps > f64::from(grid.steps) && steps <= f64::from(u32::MAX >> 1) {
            grid.steps = steps as u32;
        }
    }
    Some(grid)
}

/// A normalized data-position grid, refined only when the shown precision distinguishes more
/// positions. Derived fractions use the float baseline even when every fraction is zero or one.
pub(crate) fn normalized_grid(values: &[f64], format: &crate::numbers::Format) -> crate::payload::Quant {
    use crate::payload::{QUANT_STEPS, Quant};
    let mut grid = Quant::for_values(
        values.iter().copied().chain([0.0, 1.0]),
        values.iter().map(|v| crate::numbers::shown(*v, format)),
    )
    .expect("normalized positions have a finite scale");
    if grid.steps < QUANT_STEPS {
        grid = Quant::over([0.0, 1.0]).expect("a normalized scale is finite");
    }
    grid.resolution = None;
    grid
}

/// `extent` widened outward to a short decimal, one digit finer than the leading digit of its
/// span (of its larger end, three digits finer, when the span is zero): a derived extent never says more about the data
/// than a grid end or an axis label does, so the HTML holds no digit the author did not choose
/// (FEA-CH-37). An extent with no usable scale comes back as it was.
pub fn coarse(extent: [f64; 2]) -> [f64; 2] {
    let [lo, hi] = extent;
    if !(lo.is_finite() && hi.is_finite() && lo <= hi) {
        return extent;
    }
    let span = hi - lo;
    let big = lo.abs().max(hi.abs());
    let lead = if span > 0.0 { span } else { big };
    if !lead.is_normal() || big > 1e290 || lead < 1e-290 {
        return extent;
    }
    let e = lead.log10().floor() as i32 - if span > 0.0 { 1 } else { 3 };
    let scale = 10f64.powi(-e);
    // A decimal built from an integer and a power of ten is the nearest double to it.
    let unit = |k: f64| if e < 0 { k / scale } else { k * 10f64.powi(e) };
    let (a, b) = ((lo * scale).floor(), (hi * scale).ceil());
    if !(a.is_finite() && b.is_finite()) {
        return extent;
    }
    [unit(a), unit(b)]
}

/// `value` with every fractional number cut to six significant digits.
pub fn round_value(value: &mut Value) {
    round_value_to(value, DIGITS);
}

fn round_value_to(value: &mut Value, digits: i32) {
    let digits = digits.clamp(1, DIGITS);
    match value {
        Value::Number(n) if n.is_f64() => {
            if let Some(x) = n.as_f64().filter(|x| x.is_finite() && *x != 0.0) {
                let scale = 10f64.powi(digits - 1 - x.abs().log10().floor() as i32);
                let rounded = (x * scale).round() / scale;
                if let Some(r) = serde_json::Number::from_f64(rounded) {
                    *n = r;
                }
            }
        }
        Value::Array(list) => list.iter_mut().for_each(|v| round_value_to(v, digits)),
        Value::Object(map) => map.values_mut().for_each(|v| round_value_to(v, digits)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_coarse_extent_covers_the_data_in_short_decimals() {
        assert_eq!(coarse([100.016, 101.69]), [100.0, 101.7]);
        assert_eq!(coarse([0.0, 4321.5]), [0.0, 4400.0]);
        assert_eq!(coarse([-3.3, 91.7]), [-4.0, 92.0]);
        assert_eq!(coarse([5.0, 5.0]), [5.0, 5.0]);
        assert_eq!(coarse([100.016, 100.016]), [100.0, 100.1]);
        assert_eq!(coarse([f64::INFINITY, f64::NEG_INFINITY]).len(), 2);
    }

    #[test]
    fn numbers_keep_six_significant_digits_and_integers_stay() {
        let mut v = json!({"a": [44.99999999487305, 1234567.891, 3, 0.000123456789, 0.0]});
        round_value(&mut v);
        assert_eq!(v, json!({"a": [45.0, 1234570.0, 3, 0.000123457, 0.0]}));
    }
}
