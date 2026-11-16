//! Histogram intervals and measures, independent of chart geometry and host state.

use crate::scale::Linear;
use serde::{Deserialize, Serialize};

pub const MAX_BINS: usize = 256;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    #[default]
    Count,
    Proportion,
    Density,
}

impl Measure {
    pub fn key(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Proportion => "proportion",
            Self::Density => "density",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Count => "Count",
            Self::Proportion => "Proportion",
            Self::Density => "Density",
        }
    }
}

/// Authored histogram options. Explicit edges own their extent and interval count.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Spec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edges: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extent: Option<[f64; 2]>,
    pub measure: Measure,
}

impl Spec {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.edges.is_some() && (self.count.is_some() || self.extent.is_some()) {
            return Err("bin edges cannot be combined with count or extent");
        }
        if self.count.is_some_and(|count| !(1..=MAX_BINS).contains(&count)) {
            return Err("bin count must be between 1 and 256");
        }
        if self
            .extent
            .is_some_and(|[lo, hi]| !lo.is_finite() || !hi.is_finite() || lo >= hi)
        {
            return Err("bin extent must contain two increasing finite values");
        }
        if let Some(edges) = &self.edges {
            validate_edges(edges)?;
        }
        Ok(())
    }

    pub fn resolve(&self, values: &[f64]) -> Result<Vec<f64>, &'static str> {
        self.validate()?;
        match &self.edges {
            Some(edges) => Ok(edges.clone()),
            None => equal_edges(values, self.count, self.extent),
        }
    }

    /// Resolve authored edges exactly, or choose automatic edges at the channel's shown precision.
    pub fn resolve_shown(&self, values: &[f64], format: &crate::numbers::Format) -> Result<Vec<f64>, &'static str> {
        let edges = self.resolve(values)?;
        if self.edges.is_some() {
            Ok(edges)
        } else if let Some(extent) = self.extent {
            // Authored bounds win over the count. Widen intervals until their interior
            // boundaries fit the shown precision, keeping the equal-width catchments exact.
            for count in (1..edges.len()).rev() {
                let mut candidate = equal_edges(values, Some(count), Some(extent))?;
                let mut fits = true;
                let end = candidate.len() - 1;
                for edge in &mut candidate[1..end] {
                    let shown = crate::numbers::shown(*edge, format);
                    let float_error = edge.abs().max(shown.abs()) * f64::EPSILON * 8.0;
                    if (*edge - shown).abs() > float_error {
                        fits = false;
                        break;
                    }
                    *edge = shown;
                }
                if fits && validate_edges(&candidate).is_ok() {
                    return Ok(candidate);
                }
            }
            Err("No bin intervals fit the authored extent")
        } else {
            shown_edges(&edges, format)
        }
    }
}

/// Choose equal-width edges before counting, with every edge exactly representable at `format`.
/// Widen the width on the common decimal unit until the last edge covers the original range.
pub fn shown_edges(edges: &[f64], format: &crate::numbers::Format) -> Result<Vec<f64>, &'static str> {
    validate_edges(edges)?;
    let (first, last) = (edges[0], edges[edges.len() - 1]);
    let magnitude = first.abs().max(last.abs());
    let exponent = |value: f64| -> i32 {
        format!("{:e}", value.abs())
            .split_once('e')
            .and_then(|(_, e)| e.parse().ok())
            .unwrap_or(0)
    };
    let first_power =
        (exponent(magnitude) - i32::from(crate::numbers::effective_digits(magnitude, format.digits)) + 1).max(-323);
    let count = edges.len() - 1;
    for power in first_power..first_power + 20 {
        let unit = format!("1e{power}").parse::<f64>().unwrap_or(0.0);
        if !unit.is_finite() || unit <= 0.0 {
            break;
        }
        let low = (first / unit).floor();
        let width = ((last / unit - low) / count as f64).ceil().max(1.0);
        let mut chosen = Vec::with_capacity(edges.len());
        for i in 0..=count {
            let coefficient = low + i as f64 * width;
            let edge = format!("{coefficient}e{power}")
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite());
            let Some(edge) = edge else {
                return Err("No finite bin edges fit the chosen precision");
            };
            chosen.push(edge);
        }
        if validate_edges(&chosen).is_ok()
            && chosen[0] <= first
            && chosen[count] >= last
            && chosen.iter().all(|v| crate::numbers::shown(*v, format) == *v)
        {
            return Ok(chosen);
        }
    }
    Err("No increasing bin edges fit the chosen precision")
}

/// Equal-width edges over an explicit extent or the observed finite range. Automatic
/// counts use Sturges' rule; constant data defaults to one finite interval. Near machine
/// precision, repeated edges are removed rather than creating zero-width intervals.
pub fn equal_edges(values: &[f64], count: Option<usize>, extent: Option<[f64; 2]>) -> Result<Vec<f64>, &'static str> {
    if count.is_some_and(|count| !(1..=MAX_BINS).contains(&count)) {
        return Err("bin count must be between 1 and 256");
    }
    if extent.is_some_and(|[lo, hi]| !lo.is_finite() || !hi.is_finite() || lo >= hi) {
        return Err("bin extent must contain two increasing finite values");
    }
    let (mut lo, mut hi, mut accepted) = (f64::INFINITY, f64::NEG_INFINITY, 0usize);
    for &value in values.iter().filter(|value| value.is_finite()) {
        if extent.is_none_or(|[lo, hi]| value >= lo && value <= hi) {
            lo = lo.min(value);
            hi = hi.max(value);
            accepted += 1;
        }
    }
    let constant = lo == hi;
    if let Some([a, b]) = extent {
        (lo, hi) = (a, b);
    } else if accepted == 0 {
        return Err("no finite observations to bin");
    } else if constant {
        let ticks = crate::scale::nice_ticks(lo, hi, 1);
        (lo, hi) = (ticks.lo, ticks.hi);
    }
    let automatic = if constant && extent.is_none() {
        1
    } else {
        ((accepted.max(1) as f64).log2().ceil() as usize + 1).min(MAX_BINS)
    };
    let count = count.unwrap_or(automatic);
    let mapping = Linear::new((lo, hi), (0.0, count as f64));
    let mut edges = Vec::with_capacity(count + 1);
    for i in 0..=count {
        let value = if i == count {
            hi
        } else {
            mapping.invert(i as f64).clamp(lo, hi)
        };
        if edges.last().is_none_or(|&previous| previous < value) {
            edges.push(value);
        }
    }
    validate_edges(&edges)?;
    Ok(edges)
}

pub fn validate_edges(edges: &[f64]) -> Result<(), &'static str> {
    if !(2..=MAX_BINS + 1).contains(&edges.len()) {
        return Err("provide between 2 and 257 bin edges");
    }
    if edges.iter().any(|value| !value.is_finite()) || edges.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("bin edges must be finite and strictly increasing");
    }
    Ok(())
}

/// Lower-inclusive, upper-exclusive intervals; the last upper edge is included.
/// The caller validates edges once before assigning its observations.
pub fn bin_index(edges: &[f64], value: f64) -> Option<usize> {
    if edges.len() < 2 || !value.is_finite() || value < edges[0] || value > *edges.last()? {
        return None;
    }
    Some(
        edges
            .partition_point(|edge| *edge <= value)
            .saturating_sub(1)
            .min(edges.len() - 2),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_options_reject_ambiguous_binning_and_unknown_measures() {
        for options in [
            serde_json::json!({"edges":[0,1],"count":1}),
            serde_json::json!({"edges":[0,1],"extent":[0,1]}),
            serde_json::json!({"count":0}),
            serde_json::json!({"count":257}),
            serde_json::json!({"extent":[0,0]}),
            serde_json::json!({"edges":[0,0]}),
        ] {
            assert!(serde_json::from_value::<Spec>(options).unwrap().validate().is_err());
        }
        for options in [
            serde_json::json!({"count":1.5}),
            serde_json::json!({"count":-1}),
            serde_json::json!({"measure":"probability"}),
            serde_json::json!({"threshold":4}),
            serde_json::json!({"extent":[0,1,2]}),
        ] {
            assert!(serde_json::from_value::<Spec>(options).is_err());
        }
        for measure in [Measure::Count, Measure::Proportion, Measure::Density] {
            let spec: Spec =
                serde_json::from_value(serde_json::json!({"measure":measure.key(),"edges":[0,1,3]})).unwrap();
            assert_eq!(spec.measure, measure);
            assert_eq!(spec.resolve(&[100.0]).unwrap(), [0.0, 1.0, 3.0]);
        }
    }

    #[test]
    fn interval_boundaries_count_each_accepted_observation_once() {
        let edges = [0.0, 10.0, 20.0, 30.0];
        let samples = [-1.0, 0.0, 9.0, 10.0, 19.0, 20.0, 30.0, 31.0, f64::NAN];
        let memberships: Vec<_> = samples.into_iter().map(|value| bin_index(&edges, value)).collect();
        assert_eq!(
            memberships,
            [None, Some(0), Some(0), Some(1), Some(1), Some(2), Some(2), None, None]
        );
        let mut counts = [0; 3];
        for bin in memberships.into_iter().flatten() {
            counts[bin] += 1;
        }
        assert_eq!(counts, [2, 2, 2]);
    }

    #[test]
    fn automatic_and_authored_counts_have_explicit_extents() {
        let values: Vec<_> = (0..16).map(|value| value as f64).collect();
        assert_eq!(
            equal_edges(&values, None, None).unwrap(),
            [0.0, 3.0, 6.0, 9.0, 12.0, 15.0]
        );
        assert_eq!(
            equal_edges(&values, Some(4), Some([0.0, 8.0])).unwrap(),
            [0.0, 2.0, 4.0, 6.0, 8.0]
        );
        assert_eq!(equal_edges(&[], Some(2), Some([0.0, 8.0])).unwrap(), [0.0, 4.0, 8.0]);
        assert!(equal_edges(&[f64::NAN], None, None).is_err());
    }

    #[test]
    fn constant_and_extreme_domains_keep_finite_distinct_edges() {
        for value in [0.0, 1.0, 1e308, -f64::MAX, f64::MAX, f64::from_bits(1)] {
            let edges = equal_edges(&[value, value], None, None).unwrap();
            assert_eq!(edges.len(), 2, "{value}: {edges:?}");
            assert!(edges[0] <= value && edges[1] >= value);
            assert!(validate_edges(&edges).is_ok());
            assert_eq!(bin_index(&edges, value), Some(0));
        }
        for (lo, hi) in [
            (-f64::MAX, f64::MAX),
            (1e-308, 2e-308),
            (1.0, 1.0_f64.next_up()),
            (f64::from_bits(1), f64::from_bits(2)),
        ] {
            let edges = equal_edges(&[lo, hi], Some(MAX_BINS), None).unwrap();
            assert_eq!(edges[0], lo);
            assert_eq!(*edges.last().unwrap(), hi);
            assert!(validate_edges(&edges).is_ok());
            assert_eq!(bin_index(&edges, hi), Some(edges.len() - 2));
        }
    }

    #[test]
    fn invalid_interval_requests_are_rejected_before_allocation() {
        for count in [0, MAX_BINS + 1, usize::MAX] {
            assert!(equal_edges(&[1.0, 2.0], Some(count), None).is_err());
        }
        for extent in [[1.0, 1.0], [2.0, 1.0], [f64::NEG_INFINITY, 2.0], [0.0, f64::NAN]] {
            assert!(equal_edges(&[1.0, 2.0], None, Some(extent)).is_err());
        }
        for edges in [
            vec![],
            vec![1.0],
            vec![0.0, 0.0, 1.0],
            vec![0.0, f64::INFINITY],
            vec![1.0, -1.0],
        ] {
            assert!(validate_edges(&edges).is_err());
        }
    }
}
