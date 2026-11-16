//! Linear scales and "nice" value ticks.

/// Maps a value domain onto a pixel range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Linear {
    pub d0: f64,
    pub d1: f64,
    pub r0: f64,
    pub r1: f64,
}

impl Linear {
    pub fn new(domain: (f64, f64), range: (f64, f64)) -> Self {
        Self {
            d0: domain.0,
            d1: domain.1,
            r0: range.0,
            r1: range.1,
        }
    }

    #[inline]
    pub fn map(&self, v: f64) -> f64 {
        let span = self.d1 - self.d0;
        if span == 0.0 {
            return (self.r0 + self.r1) / 2.0;
        }
        self.r0 + position(v, self.d0, self.d1) * (self.r1 - self.r0)
    }

    pub fn invert(&self, px: f64) -> f64 {
        let span = self.r1 - self.r0;
        if span == 0.0 {
            return self.d0;
        }
        let t = (px - self.r0) / span;
        let domain_span = self.d1 - self.d0;
        if domain_span.is_infinite() && self.d0.is_finite() && self.d1.is_finite() {
            self.d0 * (1.0 - t) + self.d1 * t
        } else {
            self.d0 + t * domain_span
        }
    }
}

/// Position on an affine domain. Opposite finite endpoints can have an infinite difference;
/// halving first retains the same ratio without overflowing that intermediate subtraction.
#[inline]
pub(crate) fn position(value: f64, lo: f64, hi: f64) -> f64 {
    let span = hi - lo;
    if span.is_infinite() && lo.is_finite() && hi.is_finite() {
        (value * 0.5 - lo * 0.5) / (hi * 0.5 - lo * 0.5)
    } else {
        (value - lo) / span
    }
}

/// Evenly spaced round values covering a domain.
#[derive(Clone, Debug, PartialEq)]
pub struct NiceTicks {
    pub lo: f64,
    pub hi: f64,
    pub step: f64,
    /// Decimals needed to print every tick exactly (step 2.5 needs one, step 0.25 two).
    pub decimals: usize,
    pub values: Vec<f64>,
}

/// Round step mantissas with the power-of-ten offset of their last significant digit
/// (2.5 = 25 × 10⁻¹ needs one more decimal than 2 or 5).
const MANTISSAS: [(f64, i32); 5] = [(1.0, 0), (2.0, 0), (2.5, -1), (5.0, 0), (10.0, 1)];

/// The smallest round step that divides `span` into at most `target` intervals, with the
/// decimals needed to print its multiples exactly. With `integer`, for axes whose ticks must be
/// whole numbers (calendar years), the step is a whole number of at least one, never a 2.5
/// mantissa.
fn step_for(span: f64, target: usize, integer: bool) -> (f64, usize) {
    let raw = (span / target.max(1) as f64).abs().max(f64::MIN_POSITIVE);
    let exponent = raw.log10().floor() as i32;
    let base = 10f64.powi(exponent);
    let (mantissa, digit) = MANTISSAS
        .into_iter()
        .filter(|(m, _)| !integer || *m != 2.5)
        .find(|(m, _)| m * base >= raw * (1.0 - 1e-9))
        .unwrap_or((10.0, 1));
    if integer {
        return ((mantissa * base).round().max(1.0), 0);
    }
    let last_digit = exponent + digit;
    (
        mantissa * base,
        if last_digit >= 0 { 0 } else { (-last_digit) as usize },
    )
}

pub(crate) fn round_to(v: f64, decimals: usize) -> f64 {
    let f = 10f64.powi(decimals as i32);
    // Near the top of the range the scaled value overflows; such values have no decimals.
    if !(v * f).is_finite() {
        return v;
    }
    let r = (v * f).round() / f;
    if r == 0.0 { 0.0 } else { r }
}

/// Ticks for `[lo, hi]` extended outward to the nearest round values, so both ends of the axis
/// are labelled ticks. The step is the round step for at most `target` intervals, unless the round
/// step for one interval fewer or more leaves at least a tenth of the axis less empty beyond the
/// data (0–50 instead of 0–60 for values up to 47.9); a step of 2.5 × 10ⁿ, the least simple, must
/// save a fifth, and no alternative leaves the axis with a single interval.
pub fn nice_ticks(lo: f64, hi: f64, target: usize) -> NiceTicks {
    nice_ticks_with(lo, hi, target, false, 1)
}

/// [`nice_ticks`] for a vertical axis, which may take up to two more ticks than asked for when the
/// round step they give saves it a tenth of its length: a short plot reads better with steps of 10
/// up to 70 than with 20 up to 80 (ARC-64 boxplot, combo).
pub fn nice_ticks_finer(lo: f64, hi: f64, target: usize) -> NiceTicks {
    nice_ticks_with(lo, hi, target, false, if target <= 5 { 2 } else { 1 })
}

/// [`nice_ticks`] with whole-number steps (see [`integer_step`]), for year axes: labels can
/// neither repeat nor sit at fractional positions.
pub fn nice_integer_ticks(lo: f64, hi: f64, target: usize) -> NiceTicks {
    nice_ticks_with(lo, hi, target, true, 1)
}

fn nice_ticks_with(lo: f64, hi: f64, target: usize, integer: bool, finer: usize) -> NiceTicks {
    let (mut lo, mut hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    if !lo.is_finite() || !hi.is_finite() {
        lo = 0.0;
        hi = 1.0;
    }
    if !integer && let Some(ticks) = rescaled_ticks(lo, hi, target, false) {
        return ticks;
    }
    if hi - lo < f64::EPSILON * hi.abs().max(1.0) {
        let pad = if lo == 0.0 { 1.0 } else { lo.abs() * 0.05 };
        lo = (lo - pad).max(f64::MIN);
        hi = (hi + pad).min(f64::MAX);
    }
    let target = target.max(1);
    let snapped = |t: usize| {
        let (step, decimals) = step_for(hi - lo, t, integer);
        let k0 = (lo / step + 1e-9).floor();
        let k1 = (hi / step - 1e-9).ceil();
        let values: Vec<f64> = (0..=((k1 - k0) as i64).max(1))
            .map(|i| round_to((k0 + i as f64) * step, decimals + 2))
            .collect();
        NiceTicks {
            lo: k0 * step,
            hi: values.last().copied().unwrap_or(k1 * step),
            step,
            decimals,
            values,
        }
    };
    let empty = |t: &NiceTicks| 1.0 - (hi - lo) / (t.hi - t.lo).max(f64::MIN_POSITIVE);
    let quarter = |t: &NiceTicks| {
        let mantissa = t.step / 10f64.powf(t.step.log10().floor());
        (mantissa - 2.5).abs() < 1e-6
    };
    let base = snapped(target);
    std::iter::once(target.saturating_sub(1).max(1))
        .chain(target + 1..=target + finer)
        .map(snapped)
        .filter(|t| t.values.len() >= base.values.len().min(3))
        .filter(|t| empty(t) < empty(&base) - if quarter(t) { 0.2 } else { FINER_STEP_SAVING })
        .min_by(|a, b| empty(a).total_cmp(&empty(b)))
        .unwrap_or(base)
}

/// The share of the axis a finer or coarser round step must save beyond the data, against the step
/// the tick count asks for, to replace it.
const FINER_STEP_SAVING: f64 = 0.1;

/// The range a value axis must show before it is snapped to ticks. `zero` keeps zero in it (bars,
/// areas, percentages of one-signed data). `soft` lets a line whose values lie on one side of zero
/// extend towards zero by up to a third of its range, reaching zero when that is within the
/// reach, so the line keeps room beside it without the axis crossing zero.
pub fn extent(lo: f64, hi: f64, zero: bool, soft: bool) -> (f64, f64) {
    let (mut lo, mut hi) = (lo, hi);
    if zero {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    } else if soft {
        let reach = (hi - lo) / 3.0;
        if lo > 0.0 {
            lo = if lo - reach <= 0.0 { 0.0 } else { lo - reach };
        } else if hi < 0.0 {
            hi = if hi + reach >= 0.0 { 0.0 } else { hi + reach };
        }
    }
    (lo, hi)
}

/// Whether a unit names an interval scale, one whose zero is a convention and not a floor: degrees
/// of temperature or of angle. A line measuring them fits its data instead of reaching for zero.
pub fn is_interval_unit(unit: &str) -> bool {
    let unit = unit.trim();
    unit.starts_with('°') || unit == "℃" || unit == "℉"
}

/// The ends of an axis that does not measure from zero, between the data and the round ticks that
/// cover it: each end is the data's own, rounded outward to half a step (a quarter for a 2 step, so
/// 200 ends at 450 for data to 415), and never beyond the covering ticks `axis_lo` and `axis_hi`.
/// `room` is the distance in data units the data must keep from either end.
pub fn fitted_ends(data_lo: f64, data_hi: f64, step: f64, axis_lo: f64, axis_hi: f64, room: f64) -> (f64, f64) {
    if !(step.is_finite() && step > 0.0) {
        return (axis_lo, axis_hi);
    }
    let base = 10f64.powf(step.log10().floor());
    let sub = if step / base < 4.0 { 0.5 * base } else { 2.5 * base };
    let decimals = (1 - base.log10().floor() as i32).max(0) as usize + 1;
    let lo = round_to(((data_lo - room) / sub + 1e-9).floor() * sub, decimals).max(axis_lo);
    let hi = round_to(((data_hi + room) / sub - 1e-9).ceil() * sub, decimals).min(axis_hi);
    if hi > lo { (lo, hi) } else { (axis_lo, axis_hi) }
}

/// Ticks inside a fixed domain (no outward extension), used while a domain is animating.
pub fn ticks_within(lo: f64, hi: f64, target: usize) -> NiceTicks {
    ticks_within_with(lo, hi, target, false)
}

/// [`ticks_within`] with whole-number steps, for zoomed year axes.
pub fn integer_ticks_within(lo: f64, hi: f64, target: usize) -> NiceTicks {
    ticks_within_with(lo, hi, target, true)
}

fn ticks_within_with(lo: f64, hi: f64, target: usize, integer: bool) -> NiceTicks {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    if !integer && let Some(ticks) = rescaled_ticks(lo, hi, target, true) {
        return ticks;
    }
    let (step, decimals) = step_for((hi - lo).max(f64::MIN_POSITIVE), target, integer);
    let k0 = (lo / step - 1e-9).ceil();
    let k1 = (hi / step + 1e-9).floor();
    let count = ((k1 - k0) as i64).max(-1);
    let values = (0..=count)
        .map(|i| round_to((k0 + i as f64) * step, decimals + 2))
        .collect();
    NiceTicks {
        lo,
        hi,
        step,
        decimals,
        values,
    }
}

/// Keep ordinary domains byte-for-byte stable. Near the exponent limits, tick rounding and
/// endpoint subtraction need ordinary-sized operands; restore source units only afterwards.
fn rescaled_ticks(lo: f64, hi: f64, target: usize, within: bool) -> Option<NiceTicks> {
    let magnitude = lo.abs().max(hi.abs());
    if !lo.is_finite() || !hi.is_finite() || magnitude == 0.0 || (1e-300..=1e300).contains(&magnitude) {
        return None;
    }
    let exponent = magnitude.log10().floor() as i32;
    let factor = 10f64.powi(exponent).max(f64::from_bits(1));
    let scaled = if within {
        ticks_within(lo / factor, hi / factor, target)
    } else {
        nice_ticks(lo / factor, hi / factor, target)
    };
    let restore = |value: f64| (value * factor).clamp(-f64::MAX, f64::MAX);
    let (mut lower, mut upper) = if within {
        (lo, hi)
    } else {
        (restore(scaled.lo).min(lo), restore(scaled.hi).max(hi))
    };
    if !within && lower == upper {
        lower = lower.next_down().max(-f64::MAX);
        upper = upper.next_up().min(f64::MAX);
    }
    let mut values: Vec<f64> = scaled
        .values
        .into_iter()
        .map(restore)
        .filter(|value| *value >= lower && *value <= upper)
        .collect();
    values.dedup();
    if !within {
        if values.len() < 2 {
            values = vec![lower, upper];
        } else {
            values[0] = lower;
            *values.last_mut().unwrap() = upper;
        }
    }
    let step = (scaled.step * factor).max(f64::from_bits(1));
    Some(NiceTicks {
        lo: lower,
        hi: upper,
        step,
        decimals: (scaled.decimals as i32 - exponent).clamp(0, 324) as usize,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn steps_are_round_and_decimals_exact() {
        assert_eq!(step_for(100.0, 5, false), (20.0, 0));
        assert_eq!(step_for(10.0, 4, false), (2.5, 1));
        assert_eq!(step_for(1.0, 4, false), (0.25, 2));
        assert_eq!(step_for(0.9, 5, false), (0.2, 1));
        assert_eq!(step_for(7.0, 7, false), (1.0, 0));
        assert_eq!(step_for(95.0, 1, false), (100.0, 0));
        assert_eq!(step_for(250.0, 5, false), (50.0, 0));
        assert_eq!(step_for(0.007, 5, false), (0.002, 3));
    }

    #[test]
    fn extents_keep_zero_or_give_lines_room_towards_it() {
        assert_eq!(extent(11.1, 46.0, true, true), (0.0, 46.0));
        assert_eq!(extent(315.0, 426.0, false, true), (278.0, 426.0));
        assert_eq!(extent(2.1, 9.4, false, true), (0.0, 9.4), "zero within reach");
        assert_eq!(extent(-40.0, -10.0, false, true), (-40.0, 0.0));
        assert_eq!(extent(-3.0, 8.0, false, true), (-3.0, 8.0), "crossing data stay");
        assert_eq!(extent(315.0, 426.0, false, false), (315.0, 426.0));
    }

    #[test]
    fn ticks_cover_domain_on_round_values() {
        let t = nice_ticks(313.2, 431.1, 5);
        assert_eq!(t.step, 25.0);
        assert_eq!(t.values, vec![300.0, 325.0, 350.0, 375.0, 400.0, 425.0, 450.0]);
        assert!(t.lo <= 313.2 && t.hi >= 431.1);

        let pct = nice_ticks(-2.1, 9.06, 5);
        assert_eq!(pct.values, vec![-2.5, 0.0, 2.5, 5.0, 7.5, 10.0]);
        assert_eq!(pct.decimals, 1);

        let tiny = nice_ticks(0.1, 0.3, 4);
        assert_eq!(tiny.values, vec![0.1, 0.15, 0.2, 0.25, 0.3]);
        assert_eq!(tiny.decimals, 2);

        let flat = nice_ticks(5.0, 5.0, 4);
        assert!(flat.lo < 5.0 && flat.hi > 5.0);
    }

    #[test]
    fn axis_ends_are_ticks_without_a_wasted_band() {
        let share = nice_ticks(0.0, 47.9, 4);
        assert_eq!((share.values[0], share.values[share.values.len() - 1]), (0.0, 50.0));
        assert_eq!(nice_ticks(0.0, 21.9, 5).values, vec![0.0, 5.0, 10.0, 15.0, 20.0, 25.0]);
        assert_eq!(nice_ticks(0.0, 70.9, 7).step, 20.0, "no finer step without a saving");
        assert_eq!(nice_ticks(0.0, 11.5, 4).step, 5.0, "2.5 must save a fifth");
        // A vertical axis may take two more ticks when they save a tenth of its length.
        assert_eq!(nice_ticks_finer(0.0, 11.5, 4).step, 2.0);
        assert_eq!(
            nice_ticks_finer(0.0, 64.0, 5).values.last(),
            Some(&70.0),
            "a short plot steps by 10 up to 70, not 20 up to 80"
        );
        assert_eq!(nice_ticks(0.0, 64.0, 5).values.last(), Some(&80.0));
        assert_eq!(
            nice_ticks_finer(0.0, 21.9, 7).values,
            vec![0.0, 5.0, 10.0, 15.0, 20.0, 25.0],
            "a tall plot keeps its steps"
        );
        assert_eq!(
            nice_ticks(0.0, 22.0, 2).values,
            vec![0.0, 10.0, 20.0, 30.0],
            "never one interval"
        );
    }

    #[test]
    fn a_data_fitted_axis_ends_just_past_its_data() {
        // Scatter seats 12 to 415 on steps of 100: the axis ends at 450, not 500.
        assert_eq!(fitted_ends(0.0, 415.0, 100.0, 0.0, 500.0, 0.0), (0.0, 450.0));
        // A step of 200 ends at 450 as well (quarter steps), not at the covering 600.
        assert_eq!(fitted_ends(0.0, 415.0, 200.0, 0.0, 600.0, 0.0), (0.0, 450.0));
        // Prices 98.3 to 107.6 on steps of 2: the axis spans 98 to 108.
        assert_eq!(fitted_ends(98.3, 107.6, 2.0, 98.0, 108.0, 0.0), (98.0, 108.0));
        // Room for a bubble keeps the end clear of it.
        assert_eq!(fitted_ends(0.0, 415.0, 100.0, 0.0, 500.0, 40.0), (0.0, 500.0));
        assert!(is_interval_unit("°C") && is_interval_unit(" °F") && !is_interval_unit("k") && !is_interval_unit("%"));
    }

    #[test]
    fn fixed_domain_ticks_stay_inside() {
        let t = ticks_within(312.7, 431.9, 5);
        assert!(t.values.iter().all(|v| (312.7..=431.9).contains(v)));
        assert_eq!(t.values.first().copied(), Some(325.0));
    }

    #[test]
    fn integer_ticks_are_whole_unique_and_never_use_a_quarter_step() {
        for (lo, hi, target) in [
            (1700.0, 1780.0, 5),
            (1700.0, 1704.0, 6),
            (1700.0, 1701.0, 6),
            (1699.5, 1700.5, 6),
            (1790.0, 1810.0, 4),
            (1000.0, 2999.0, 7),
        ] {
            for ticks in [nice_integer_ticks(lo, hi, target), integer_ticks_within(lo, hi, target)] {
                assert!(!ticks.values.is_empty(), "{lo}..{hi}");
                assert!(
                    ticks.step >= 1.0 && ticks.step.fract() == 0.0,
                    "{lo}..{hi}: {}",
                    ticks.step
                );
                assert_eq!(ticks.decimals, 0);
                assert!(
                    ticks.values.iter().all(|v| v.fract() == 0.0),
                    "{lo}..{hi}: {:?}",
                    ticks.values
                );
                assert!(ticks.values.windows(2).all(|w| w[1] > w[0]));
                let mantissa = ticks.step / 10f64.powf(ticks.step.log10().floor());
                assert!((mantissa - 2.5).abs() > 1e-6, "{lo}..{hi}: step {}", ticks.step);
            }
        }
        // The ordinary scale keeps its quarter and fractional steps.
        assert_eq!(nice_ticks(0.0, 1.0, 4).step, 0.25);
        assert_eq!(nice_integer_ticks(1700.0, 1780.0, 5).step, 20.0);
        assert_eq!(nice_integer_ticks(1700.0, 1704.0, 6).step, 1.0);
        let covering = nice_integer_ticks(1701.0, 1779.0, 5);
        assert!(covering.lo <= 1701.0 && covering.hi >= 1779.0);
        assert!(
            integer_ticks_within(1702.0, 1778.0, 5)
                .values
                .iter()
                .all(|v| (1702.0..=1778.0).contains(v))
        );
    }

    #[test]
    fn linear_maps_and_inverts() {
        let s = Linear::new((0.0, 100.0), (300.0, 0.0));
        assert_close(s.map(25.0), 225.0);
        assert_close(s.invert(225.0), 25.0);
    }
}
