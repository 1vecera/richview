//! R-7 quartiles and observed whiskers; sample identifiers refer to original source rows.
use crate::scale::Linear;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub count: usize,
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
    pub lower_whisker: f64,
    pub upper_whisker: f64,
    pub outliers: Vec<usize>,
}

fn quantile(sorted: &[(usize, f64)], fraction: f64) -> f64 {
    let position = (sorted.len() - 1) as f64 * fraction;
    let index = position.floor() as usize;
    let lower = sorted[index].1;
    let upper = sorted[(index + 1).min(sorted.len() - 1)].1;
    Linear::new((lower, upper), (0.0, 1.0))
        .invert(position.fract())
        .clamp(lower, upper)
}

fn outside(value: f64, q1: f64, q3: f64) -> bool {
    let (lower, upper) = if value < q1 {
        (value, q1)
    } else if value > q3 {
        (q3, value)
    } else {
        return false;
    };
    let spread = q3 - q1;
    // An IQR wider than the finite range puts both 1.5-IQR fences beyond all finite data.
    if spread.is_infinite() {
        return false;
    }
    if spread == 0.0 {
        return true;
    }
    let distance = upper - lower;
    let ratio = if distance.is_finite() {
        distance / spread
    } else {
        (upper * 0.5 - lower * 0.5) / (spread * 0.5)
    };
    ratio > 1.5
}

/// Values must be finite; callers disclose rejected rows before passing the accepted sample.
/// Whiskers are observations inside inclusive 1.5-IQR fences. Outlier IDs keep source order.
pub fn summarize(samples: &[(usize, f64)]) -> Result<Summary, &'static str> {
    if samples.is_empty() {
        return Err("A box plot needs at least one finite observation");
    }
    if samples.iter().any(|(_, value)| !value.is_finite()) {
        return Err("Box-plot observations must be finite");
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|(ai, a), (bi, b)| a.total_cmp(b).then(ai.cmp(bi)));
    let q1 = quantile(&sorted, 0.25);
    let median = quantile(&sorted, 0.5);
    let q3 = quantile(&sorted, 0.75);
    let mut outliers = Vec::new();
    let mut whiskers: Option<[f64; 2]> = None;
    for &(index, value) in &sorted {
        if outside(value, q1, q3) {
            outliers.push(index);
        } else if let Some(bounds) = &mut whiskers {
            bounds[1] = value;
        } else {
            whiskers = Some([value, value]);
        }
    }
    let [lower_whisker, upper_whisker] = whiskers.expect("Central observations are inside their quartile fences");
    outliers.sort_unstable();
    Ok(Summary {
        count: samples.len(),
        min: sorted[0].1,
        q1,
        median,
        q3,
        max: sorted.last().unwrap().1,
        lower_whisker,
        upper_whisker,
        outliers,
    })
}
