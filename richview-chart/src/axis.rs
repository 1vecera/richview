//! Axes: value ranges, tick density and label text (FEA-CH-19, FEA-CH-21 to FEA-CH-26,
//! FEA-CH-73).

use crate::numbers::{self, Format, Kind, Locale};
use crate::scale;
use crate::text::Font;
use crate::time::{self, Interval, Precision, Unit};

/// How a value axis treats zero (FEA-CH-19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zero {
    /// Bars, areas, histograms and one-signed percentages include zero.
    Include,
    /// Lines get up to a third of their range of room towards zero, and reach it when that room
    /// would get there.
    Soft,
    /// Scatter axes keep to the data.
    None,
}

/// What a value axis needs to lay itself out.
#[derive(Debug, Clone)]
pub struct ValueSpec<'a> {
    /// Smallest and largest value to show.
    pub data: [f64; 2],
    pub zero: Zero,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Axis length in pixels.
    pub length: f64,
    /// True when the axis runs horizontally, so labels sit side by side.
    pub horizontal: bool,
    pub format: &'a Format,
    pub font: &'a Font,
    pub size: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValueAxis {
    pub lo: f64,
    pub hi: f64,
    pub step: f64,
    pub ticks: Vec<f64>,
    pub labels: Vec<String>,
    /// The unit the tick labels could not carry at any step; the caller names it in the title.
    pub unit_dropped: Option<String>,
}

impl ValueAxis {
    /// Pixel position of `v` on a range from `r0` (at `lo`) to `r1` (at `hi`).
    pub fn map(&self, v: f64, r0: f64, r1: f64) -> f64 {
        scale::Linear::new((self.lo, self.hi), (r0, r1)).map(v)
    }
    pub fn widest(&self, font: &Font, size: f64) -> f64 {
        self.labels.iter().map(|l| font.width(l, size)).fold(0.0, f64::max)
    }
}

/// Pixels per label along an axis: about one label per 90 px horizontally and 52 px vertically
/// (FEA-CH-24).
pub const H_SPACING: f64 = 90.0;
pub const V_SPACING: f64 = 52.0;
/// The densest a short value axis is asked to run, and the tick count it is asked to reach.
const SHORT_PLOT_SPACING: f64 = 30.0;
const SHORT_PLOT_TICKS: usize = 5;

/// Lay out a value axis: the range follows the zero rule and authored bounds, ends on labelled
/// ticks, and the tick count follows the axis length, never closer than the widest label plus
/// one em (FEA-CH-19, FEA-CH-24).
pub fn value_axis(spec: &ValueSpec) -> ValueAxis {
    value_axis_room(spec, 0.0)
}

/// [`value_axis`] for marks with a size of their own (bubbles) or a label past their value: an
/// axis that fits its data keeps `room` pixels clear beyond the first and last mark, so no disk
/// crosses the axis line, and an axis from zero keeps `room` pixels above its largest value.
pub fn value_axis_room(spec: &ValueSpec, room: f64) -> ValueAxis {
    // Public engine callers may pass non-finite authored bounds; treat them as absent
    // rather than letting one reach a scale or clamp as an invalid endpoint.
    let normalized = ValueSpec {
        data: if spec.data[0].is_finite() && spec.data[1].is_finite() && spec.data[0] <= spec.data[1] {
            spec.data
        } else {
            [0.0, 1.0]
        },
        min: spec.min.filter(|v| v.is_finite()),
        max: spec.max.filter(|v| v.is_finite()),
        ..spec.clone()
    };
    let spec = &normalized;
    let [mut lo, mut hi] = spec.data;
    if !(lo.is_finite() && hi.is_finite()) || lo > hi {
        lo = 0.0;
        hi = 1.0;
    }
    let percent_one_signed = spec.format.kind == Kind::Percent && (lo >= 0.0 || hi <= 0.0);
    let zero = if percent_one_signed {
        Zero::Include
    } else if spec.zero == Zero::Soft && spec.format.unit.as_deref().is_some_and(scale::is_interval_unit) {
        // A temperature's zero is a convention: the line fits its data, as on every other axis
        // that does not measure from zero.
        Zero::None
    } else {
        spec.zero
    };
    let (elo, ehi) = match zero {
        Zero::Include => scale::extent(lo, hi, true, false),
        Zero::Soft => scale::extent(lo, hi, false, true),
        Zero::None => (lo, hi),
    };
    let (mut lo, mut hi) = (spec.min.unwrap_or(elo), spec.max.unwrap_or(ehi));
    if lo > hi {
        match (spec.min, spec.max) {
            (Some(_), None) => hi = lo,
            (None, Some(_)) => lo = hi,
            _ => std::mem::swap(&mut lo, &mut hi),
        }
    }
    if lo >= hi {
        // A flat series: open a small window around its value without crossing zero.
        let pad = if lo == 0.0 {
            1.0
        } else {
            (lo.abs() * 0.1).max(f64::MIN_POSITIVE)
        };
        if spec.min.is_some() && spec.max.is_none() {
            hi = (lo + pad).min(f64::MAX);
            if hi <= lo {
                lo = (lo - pad).max(f64::MIN);
            }
        } else if spec.max.is_some() && spec.min.is_none() {
            lo = (hi - pad).max(f64::MIN);
            if lo >= hi {
                hi = (hi + pad).min(f64::MAX);
            }
        } else if lo >= 0.0 && lo - pad < 0.0 && spec.min.is_none() {
            hi = lo + pad;
            lo = 0.0_f64.max(lo - pad);
        } else {
            // Never past the representable range (a flat series at ±1.8e308).
            lo = (lo - pad).max(f64::MIN);
            hi = (hi + pad).min(f64::MAX);
        }
    }
    let spacing = if spec.horizontal { H_SPACING } else { V_SPACING };
    let integer = spec.format.kind == Kind::Year;
    let mut target = ((spec.length / spacing).floor() as usize).clamp(1, 12);
    if !spec.horizontal {
        // A short plot still needs steps a reader can estimate between: at least five, where the
        // labels clear each other (ARC-64 boxplot, combo).
        target = target.max(((spec.length / SHORT_PLOT_SPACING).floor() as usize).min(SHORT_PLOT_TICKS));
    }
    let fixed = spec.min.is_some() || spec.max.is_some();
    loop {
        let ticks = if fixed {
            let t = if integer {
                scale::integer_ticks_within(lo, hi, target)
            } else {
                scale::ticks_within(lo, hi, target)
            };
            // Authored bounds are the axis ends.
            scale::NiceTicks { lo, hi, ..t }
        } else if integer {
            scale::nice_integer_ticks(lo, hi, target)
        } else if spec.horizontal {
            scale::nice_ticks(lo, hi, target)
        } else {
            scale::nice_ticks_finer(lo, hi, target)
        };
        let mut values = ticks.values.clone();
        // An axis never crosses zero unless the data or an authored bound does (FEA-CH-19).
        if spec.data[0] >= 0.0 && spec.min.is_none_or(|m| m >= 0.0) {
            values.retain(|v| *v >= -1e-12);
        }
        if spec.data[1] <= 0.0 && spec.max.is_none_or(|m| m <= 0.0) {
            values.retain(|v| *v <= 1e-12);
        }
        let (mut alo, mut ahi) = if fixed {
            (lo, hi)
        } else {
            (
                values.first().copied().unwrap_or(ticks.lo),
                values.last().copied().unwrap_or(ticks.hi),
            )
        };
        if zero == Zero::None && !fixed {
            // An axis that does not measure from zero ends just past its data, not a step past.
            let (flo, fhi) = scale::fitted_ends(
                lo,
                hi,
                ticks.step,
                alo,
                ahi,
                room * (hi - lo) / (spec.length - 2.0 * room).max(1.0),
            );
            let inside: Vec<f64> = values
                .iter()
                .copied()
                .filter(|v| *v >= flo - 1e-12 && *v <= fhi + 1e-12)
                .collect();
            if inside.len() >= 2 {
                values = inside;
                (alo, ahi) = (flo, fhi);
            }
        }
        if room > 0.0 && !fixed && zero != Zero::None && alo < ahi {
            // Marks with a label or head past their value keep `room` pixels above the largest:
            // one more tick when the last would crowd it.
            let need = room * (ahi - alo) / (spec.length - room).max(1.0);
            if ahi < hi + need - 1e-9 && ticks.step.is_finite() {
                ahi = scale::round_to(ahi + ticks.step, 12);
                values.push(ahi);
            }
        }
        let labels = numbers::axis_labels(&values, ticks.step, spec.format);
        let axis = ValueAxis {
            lo: alo,
            hi: ahi,
            step: ticks.step,
            ticks: values,
            labels,
            unit_dropped: None,
        };
        if fits(spec, &axis) {
            return axis;
        }
        if target <= 1 {
            return coarser(spec, axis, fixed);
        }
        target -= 1;
    }
}

/// Whether the tick labels clear each other: a step of `axis.step` over its pixels is at least the
/// widest measured label plus one em horizontally, or a line and a half-em vertically.
fn fits(spec: &ValueSpec, axis: &ValueAxis) -> bool {
    if axis.ticks.len() < 2 {
        return true;
    }
    let span = (axis.hi - axis.lo).abs().max(f64::MIN_POSITIVE);
    let px = spec.length * axis.step / span;
    let need = if spec.horizontal {
        axis.widest(spec.font, spec.size) + spec.size
    } else {
        spec.font.line_height(spec.size) + spec.size * 0.5
    };
    px >= need
}

/// The next step of the 1, 2, 5 series above `step`.
fn next_step(step: f64) -> f64 {
    let magnitude = 10f64.powf(step.log10().floor());
    let m = step / magnitude;
    magnitude
        * if m < 1.5 {
            2.0
        } else if m < 3.5 {
            5.0
        } else {
            10.0
        }
}

/// Ticks at multiples of `step` over `lo..hi` (`fixed`: only those inside the authored bounds).
fn stepped(spec: &ValueSpec, lo: f64, hi: f64, step: f64, fixed: bool, format: &Format) -> ValueAxis {
    let k0 = (lo / step + 1e-9).floor();
    let k1 = (hi / step - 1e-9).ceil().max(k0 + 1.0);
    let mut values: Vec<f64> = (0..=((k1 - k0) as i64))
        .map(|i| scale::round_to((k0 + i as f64) * step, 12))
        .collect();
    if fixed {
        values.retain(|v| *v >= lo - 1e-12 && *v <= hi + 1e-12);
    }
    if spec.data[0] >= 0.0 && spec.min.is_none_or(|m| m >= 0.0) {
        values.retain(|v| *v >= -1e-12);
    }
    if spec.data[1] <= 0.0 && spec.max.is_none_or(|m| m <= 0.0) {
        values.retain(|v| *v <= 1e-12);
    }
    let (alo, ahi) = if fixed {
        (lo, hi)
    } else {
        (
            values.first().copied().unwrap_or(lo),
            values.last().copied().unwrap_or(hi),
        )
    };
    let labels = numbers::axis_labels(&values, step, format);
    ValueAxis {
        lo: alo,
        hi: ahi,
        step,
        ticks: values,
        labels,
        unit_dropped: None,
    }
}

/// Labels that still collide at the smallest tick count: raise the step through 1, 2, 5, 10 ...
/// until they clear each other, and when even two do not fit with their unit, drop the unit from
/// the ticks and hand it to the title. An axis never leaves colliding labels that a larger step
/// would have separated.
fn coarser(spec: &ValueSpec, first: ValueAxis, fixed: bool) -> ValueAxis {
    let (lo, hi) = (first.lo, first.hi);
    let search = |format: &Format| -> Option<ValueAxis> {
        let mut step = first.step;
        for _ in 0..40 {
            step = next_step(step);
            if !step.is_finite() {
                return None;
            }
            let axis = stepped(spec, lo, hi, step, fixed, format);
            if fits(spec, &axis) {
                return Some(axis);
            }
        }
        None
    };
    if let Some(axis) = search(spec.format) {
        return axis;
    }
    if let Some(unit) = &spec.format.unit {
        let bare = Format {
            unit: None,
            ..spec.format.clone()
        };
        let mut axis = search(&bare).unwrap_or(first);
        axis.unit_dropped = Some(unit.clone());
        return axis;
    }
    first
}

// ---------------------------------------------------------------------------------------------
// Calendar axes (FEA-CH-15, FEA-CH-24, FEA-CH-25).

fn month_name(month: u32, locale: Locale) -> &'static str {
    let i = month as usize - 1;
    match locale {
        Locale::CsCz => [
            "led", "úno", "bře", "dub", "kvě", "čvn", "čvc", "srp", "zář", "říj", "lis", "pro",
        ][i],
        Locale::DeDe => [
            "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
        ][i],
        _ => [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ][i],
    }
}

/// A month label, with its four-digit year when `with_year`.
pub fn month_label(year: i64, month: u32, locale: Locale, with_year: bool) -> String {
    match (locale, with_year) {
        (Locale::JaJp, true) => format!("{year}年{month}月"),
        (Locale::JaJp, false) => format!("{month}月"),
        (_, true) => format!("{} {year}", month_name(month, locale)),
        (_, false) => month_name(month, locale).to_owned(),
    }
}

/// A day label, with its four-digit year when `with_year`.
pub fn day_label(year: i64, month: u32, day: u32, locale: Locale, with_year: bool) -> String {
    let m = month_name(month, locale);
    match (locale, with_year) {
        (Locale::EnUs, true) => format!("{m} {day}, {year}"),
        (Locale::EnUs, false) => format!("{m} {day}"),
        (Locale::EnGb, true) => format!("{day} {m} {year}"),
        (Locale::EnGb, false) => format!("{day} {m}"),
        (Locale::CsCz | Locale::DeDe, true) => format!("{day}. {m} {year}"),
        (Locale::CsCz | Locale::DeDe, false) => format!("{day}. {m}"),
        (Locale::JaJp, true) => format!("{year}年{month}月{day}日"),
        (Locale::JaJp, false) => format!("{month}月{day}日"),
    }
}

/// The full date a readout shows, at the precision the source wrote (FEA-CH-25).
pub fn full_date(ms: i64, precision: Precision, locale: Locale) -> String {
    let c = time::civil(ms);
    match precision {
        Precision::Year => c.year.to_string(),
        Precision::Month => month_label(c.year, c.month, locale, true),
        Precision::Day => day_label(c.year, c.month, c.day, locale, true),
        Precision::Minute => format!(
            "{} {:02}:{:02}",
            day_label(c.year, c.month, c.day, locale, true),
            c.hour,
            c.minute
        ),
        Precision::Second | Precision::Millisecond => {
            format!(
                "{} {:02}:{:02}:{:02}",
                day_label(c.year, c.month, c.day, locale, true),
                c.hour,
                c.minute,
                c.second
            )
        }
    }
}

/// Labels for calendar ticks on one row: the first label and every label where the year changes
/// carry the four-digit year; the others show only the period (FEA-CH-25).
pub fn calendar_labels(ticks: &[i64], interval: Interval, locale: Locale) -> Vec<String> {
    let mut previous: Option<time::Civil> = None;
    ticks
        .iter()
        .map(|&ms| {
            let c = time::civil(ms);
            let year_changes = previous.is_none_or(|p| p.year != c.year);
            let day_changes = previous.is_none_or(|p| (p.year, p.month, p.day) != (c.year, c.month, c.day));
            previous = Some(c);
            match interval.unit {
                Unit::Year => c.year.to_string(),
                Unit::Month => month_label(c.year, c.month, locale, year_changes),
                Unit::Week | Unit::Day => day_label(c.year, c.month, c.day, locale, year_changes),
                Unit::Hour | Unit::Minute => {
                    let clock = format!("{:02}:{:02}", c.hour, c.minute);
                    if day_changes {
                        format!("{} {clock}", day_label(c.year, c.month, c.day, locale, year_changes))
                    } else {
                        clock
                    }
                }
                Unit::Second => {
                    let clock = format!("{:02}:{:02}:{:02}", c.hour, c.minute, c.second);
                    if day_changes {
                        format!("{} {clock}", day_label(c.year, c.month, c.day, locale, year_changes))
                    } else {
                        clock
                    }
                }
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalendarAxis {
    pub interval: Interval,
    pub ticks: Vec<i64>,
    pub labels: Vec<String>,
}

/// Calendar ticks for the window `[from, to]` in UTC milliseconds: about one label per 90 px,
/// never closer than the widest label plus one em, never finer than the data's own spacing,
/// and on period starts (FEA-CH-24). The same data at the same width always gets the same ticks.
#[allow(clippy::too_many_arguments)]
pub fn calendar_axis(
    from: i64,
    to: i64,
    data_spacing: i64,
    precision: Precision,
    length: f64,
    font: &Font,
    size: f64,
    locale: Locale,
) -> CalendarAxis {
    let span = (to - from).max(1);
    let floor_unit = match precision {
        Precision::Year => Some(Unit::Year),
        Precision::Month => Some(Unit::Month),
        Precision::Day => Some(Unit::Day),
        _ => None,
    };
    let finest_ok = |iv: &Interval| {
        let not_finer_than_data = iv.min_duration_ms() >= (data_spacing as f64 * 0.9) as i64;
        let unit_ok = match floor_unit {
            Some(Unit::Year) => iv.unit == Unit::Year,
            Some(Unit::Month) => matches!(iv.unit, Unit::Month | Unit::Year),
            Some(Unit::Day) => !matches!(iv.unit, Unit::Hour | Unit::Minute | Unit::Second),
            _ => true,
        };
        not_finer_than_data && unit_ok
    };
    let target = (length / H_SPACING).floor().max(1.0) as usize + 1;
    // Labels are centred on their ticks: neighbours need half of each width plus one em.
    let px_per_ms = length / span as f64;
    let clear = |axis: &CalendarAxis| {
        axis.ticks.windows(2).zip(axis.labels.windows(2)).all(|(t, l)| {
            let gap = (t[1] - t[0]) as f64 * px_per_ms;
            gap >= (font.width(&l[0], size) + font.width(&l[1], size)) / 2.0 + size
        })
    };
    let mut fallback: Option<CalendarAxis> = None;
    let mut single: Option<CalendarAxis> = None;
    for interval in time::INTERVALS.into_iter().filter(finest_ok) {
        let ticks = interval.ticks(from, to);
        if ticks.is_empty() {
            continue;
        }
        let labels = calendar_labels(&ticks, interval, locale);
        let axis = CalendarAxis {
            interval,
            ticks,
            labels,
        };
        if axis.ticks.len() > target {
            fallback.get_or_insert(axis);
            continue;
        }
        // One label says where nothing else is: a coarser interval or a stride does better.
        if axis.ticks.len() < 2 {
            single.get_or_insert(axis);
            continue;
        }
        if clear(&axis) {
            return axis;
        }
    }
    // No round interval gives two labels that clear each other: every k-th tick of the finest
    // interval, from the first, ending on the last tick when k divides the gaps evenly, so a week
    // of days reads 1, 3, 5 and 7 (never a lone label).
    if let Some(base) = &fallback {
        let n = base.ticks.len();
        let strides = |even: bool| (2..n).filter(move |k| !even || (n - 1) % k == 0);
        for k in strides(true).chain(strides(false)) {
            let ticks: Vec<i64> = base.ticks.iter().step_by(k).copied().collect();
            if ticks.len() < 2 || ticks.len() > target {
                continue;
            }
            let labels = calendar_labels(&ticks, base.interval, locale);
            let axis = CalendarAxis {
                interval: base.interval,
                ticks,
                labels,
            };
            if clear(&axis) {
                return axis;
            }
        }
    }
    // Even the coarsest interval is too dense: keep the coarsest, thinned to fit.
    let mut axis = single.or(fallback).unwrap_or(CalendarAxis {
        interval: time::INTERVALS[time::INTERVALS.len() - 1],
        ticks: vec![from],
        labels: vec![],
    });
    if axis.labels.is_empty() {
        axis.labels = calendar_labels(&axis.ticks, axis.interval, locale);
    }
    axis
}

// ---------------------------------------------------------------------------------------------
// Ordered period categories (FEA-CH-26, FEA-CH-73).

/// The kind of ordered sequence a set of category names forms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Year,
    Quarter,
    Month,
    Day,
    /// Numbered names such as `1, 2, 3` or `Day 1, Day 2`.
    Number,
}

impl Period {
    /// Thinning steps, each a whole number of the next larger period where one exists.
    pub fn steps(self) -> &'static [usize] {
        match self {
            Period::Year => &[2, 5, 10, 20, 25, 50, 100],
            Period::Quarter => &[2, 4, 8, 20, 40],
            Period::Month => &[2, 3, 6, 12, 24, 60, 120],
            Period::Day => &[2, 7, 14, 28],
            Period::Number => &[2, 5, 10, 20, 50, 100, 200, 500, 1000],
        }
    }
    pub fn noun(self) -> &'static str {
        match self {
            Period::Year => "year",
            Period::Quarter => "quarter",
            Period::Month => "month",
            Period::Day => "day",
            Period::Number => "number",
        }
    }
}

const MONTH_WORDS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

fn month_word(word: &str) -> Option<u32> {
    let word = word.trim_end_matches('.').to_lowercase();
    (word.len() >= 3)
        .then(|| MONTH_WORDS.iter().position(|m| m.starts_with(&word)))
        .flatten()
        .map(|i| i as u32 + 1)
}

fn year_text(text: &str) -> Option<i64> {
    (text.len() == 4 && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// One period position: its kind, its index (multiples of a step fall on calendar boundaries),
/// and its year and month or day for reader text.
#[derive(Debug, Clone, Copy)]
struct Position {
    period: Period,
    index: i64,
    year: Option<i64>,
    month: u32,
    day: u32,
}

fn position(name: &str) -> Option<Position> {
    let name = name.trim();
    if let Some(y) = year_text(name) {
        return Some(Position {
            period: Period::Year,
            index: y,
            year: Some(y),
            month: 1,
            day: 1,
        });
    }
    // Quarters: `2025 Q3`, `2025-Q3`, `2025Q3`, `Q3 2025`.
    let compact: String = name
        .to_uppercase()
        .chars()
        .filter(|c| !matches!(c, ' ' | '-'))
        .collect();
    if compact.len() == 6 && compact.is_ascii() {
        let (y, q) = match compact.find('Q') {
            Some(4) => (&compact[..4], &compact[5..]),
            Some(0) => (&compact[2..], &compact[1..2]),
            _ => ("", ""),
        };
        if let (Some(y), Ok(q @ 1..=4)) = (year_text(y), q.parse::<i64>()) {
            return Some(Position {
                period: Period::Quarter,
                index: y * 4 + q - 1,
                year: Some(y),
                month: q as u32,
                day: 1,
            });
        }
    }
    if name.len() <= 10
        && let Some((ms, precision)) = time::parse_iso(name)
    {
        let c = time::civil(ms);
        return match precision {
            Precision::Month => Some(Position {
                period: Period::Month,
                index: c.year * 12 + c.month as i64 - 1,
                year: Some(c.year),
                month: c.month,
                day: 1,
            }),
            Precision::Day => Some(Position {
                period: Period::Day,
                index: time::days_from_civil(c.year, c.month, c.day) + 3,
                year: Some(c.year),
                month: c.month,
                day: c.day,
            }),
            _ => None,
        };
    }
    let mut words = name.split_whitespace();
    if let (Some(first), second, None) = (words.next(), words.next(), words.next())
        && let Some(m) = month_word(first)
    {
        return match second {
            None => Some(Position {
                period: Period::Month,
                index: m as i64 - 1,
                year: None,
                month: m,
                day: 1,
            }),
            Some(y) => year_text(y).map(|y| Position {
                period: Period::Month,
                index: y * 12 + m as i64 - 1,
                year: Some(y),
                month: m,
                day: 1,
            }),
        };
    }
    None
}

/// The integer inside a numbered name and the text around it: `Day 12` → (`Day `, 12, ``).
fn numbered(name: &str) -> Option<(&str, i64, &str)> {
    let start = name.find(|c: char| c.is_ascii_digit())?;
    let end = start
        + 1
        + name[start + 1..]
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(name.len() - start - 1);
    let n = name[start..end].parse().ok()?;
    let (prefix, suffix) = (&name[..start], &name[end..]);
    (!suffix.contains(|c: char| c.is_ascii_digit())).then_some((prefix, n, suffix))
}

/// Category names that form one ordered period sequence.
#[derive(Debug, Clone, PartialEq)]
pub struct Ordered {
    pub period: Period,
    pub index: Vec<i64>,
    /// Reader forms under the calendar label rule: four-digit year where it changes.
    pub text: Vec<String>,
    pub rising: bool,
}

/// Read category names as an ordered period sequence, if they form one (FEA-CH-73).
pub fn ordered(names: &[String], locale: Locale) -> Option<Ordered> {
    if names.is_empty() {
        return None;
    }
    let parsed: Option<Vec<Position>> = names.iter().map(|n| position(n)).collect();
    let (period, index, text) = match parsed {
        Some(all) if all.iter().all(|p| p.period == all[0].period) => {
            let period = all[0].period;
            let mut previous_year: Option<i64> = None;
            let text = all
                .iter()
                .zip(names)
                .map(|(p, name)| {
                    let changes = p.year.is_some() && p.year != previous_year;
                    previous_year = p.year.or(previous_year);
                    match (period, p.year) {
                        (Period::Year, _) => name.trim().to_owned(),
                        (Period::Quarter, Some(y)) => {
                            if changes {
                                format!("Q{} {y}", p.month)
                            } else {
                                format!("Q{}", p.month)
                            }
                        }
                        (Period::Month, Some(y)) => month_label(y, p.month, locale, changes),
                        (Period::Month, None) => month_label(0, p.month, locale, false),
                        (Period::Day, Some(y)) => day_label(y, p.month, p.day, locale, changes),
                        _ => name.trim().to_owned(),
                    }
                })
                .collect();
            (period, all.iter().map(|p| p.index).collect::<Vec<_>>(), text)
        }
        _ => {
            let parts: Vec<(&str, i64, &str)> = names.iter().map(|n| numbered(n)).collect::<Option<_>>()?;
            if parts.iter().any(|(p, _, s)| *p != parts[0].0 || *s != parts[0].2) {
                return None;
            }
            (
                Period::Number,
                parts.iter().map(|(_, n, _)| *n).collect(),
                names.to_vec(),
            )
        }
    };
    let rising = index.windows(2).all(|w| w[0] < w[1]);
    Some(Ordered {
        period,
        index,
        text,
        rising,
    })
}

/// How a category axis prints its names (FEA-CH-26).
#[derive(Debug, Clone, PartialEq)]
pub enum CategoryLayout {
    /// Every name on one line.
    OneLine,
    /// Names wrapped to at most two balanced lines.
    Wrapped(Vec<Vec<String>>),
    /// Ordered categories labelled on every `every`-th period boundary, or names labelled every
    /// `every`-th; a name with no lines is not printed.
    Thinned { every: usize, lines: Vec<Vec<String>> },
    /// Nothing fits: every name stays on one line and the unfit ones are reported.
    Unfit { lines: Vec<Vec<String>>, unfit: Vec<usize> },
}

/// How many names apart the printed names of a thinned axis stand, as a reader counts them:
/// `2nd`, `3rd`, `5th`. The nearest two decide when the spacing is uneven.
pub fn nth_printed(lines: &[Vec<String>]) -> String {
    let printed: Vec<usize> = (0..lines.len()).filter(|&i| !lines[i].is_empty()).collect();
    let apart = printed.windows(2).map(|w| w[1] - w[0]).min().unwrap_or(1);
    let suffix = match (apart % 10, apart % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{apart}{suffix}")
}

/// Choose the first layout that fits names centred in bands `band` pixels wide.
pub fn category_layout(
    names: &[String],
    ordered: Option<&Ordered>,
    band: f64,
    font: &Font,
    size: f64,
) -> CategoryLayout {
    category_layout_with(names, ordered, band, font, size, false)
}

/// [`category_layout`] for axes that cannot turn their marks sideways (lines, areas, points,
/// vertical bars once their orientation is set): names that fit neither a band nor two lines take
/// two alternating rows, so every name stays and each has two bands to itself. An empty line holds
/// a row open.
pub fn category_layout_with(
    names: &[String],
    ordered: Option<&Ordered>,
    band: f64,
    font: &Font,
    size: f64,
    stagger: bool,
) -> CategoryLayout {
    let gap = size * 0.5;
    if names.iter().all(|n| font.width(n, size) + gap <= band) {
        return CategoryLayout::OneLine;
    }
    let wrapped: Vec<Vec<String>> = names.iter().map(|n| font.wrap(n, size, band - gap, 2)).collect();
    if wrapped
        .iter()
        .all(|lines| lines.len() <= 2 && font.lines_width(lines, size) + gap <= band)
    {
        return CategoryLayout::Wrapped(wrapped);
    }
    if let Some(o) = ordered.filter(|o| o.rising) {
        for &every in o.period.steps() {
            let first = o.index[0].rem_euclid(every as i64);
            let shown: Vec<usize> = (0..names.len())
                .filter(|&i| o.index[i].rem_euclid(every as i64) == first)
                .collect();
            // A step counts in the names' own numbers, so decades ten apart all fall on "every
            // 5": a step that leaves every name is no thinning, and each printed name has the
            // bands up to the nearest printed one, not `every` of them.
            let apart = shown.windows(2).map(|w| w[1] - w[0]).min().unwrap_or(1);
            let room = band * apart as f64;
            if shown.len() >= 2
                && shown.len() < names.len()
                && shown.iter().all(|&i| font.width(&o.text[i], size) + gap <= room)
            {
                let lines = (0..names.len())
                    .map(|i| {
                        if shown.contains(&i) {
                            vec![o.text[i].clone()]
                        } else {
                            vec![]
                        }
                    })
                    .collect();
                return CategoryLayout::Thinned { every, lines };
            }
        }
    }
    if stagger && names.len() > 1 && names.iter().all(|n| font.width(n, size) + gap <= 2.0 * band) {
        let rows = names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                if i % 2 == 0 {
                    vec![n.clone()]
                } else {
                    vec![String::new(), n.clone()]
                }
            })
            .collect();
        return CategoryLayout::Wrapped(rows);
    }
    let unfit = wrapped
        .iter()
        .enumerate()
        .filter(|(_, l)| l.len() > 2 || font.lines_width(l, size) + gap > band)
        .map(|(i, _)| i)
        .collect();
    CategoryLayout::Unfit { lines: wrapped, unfit }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Font {
        Font::new("Geist Mono")
    }

    #[test]
    fn lines_get_room_towards_zero_and_bars_start_at_zero() {
        let f = Format::default();
        let line = value_axis(&ValueSpec {
            data: [400.0, 430.0],
            zero: Zero::Soft,
            min: None,
            max: None,
            length: 300.0,
            horizontal: false,
            format: &f,
            font: &font(),
            size: 12.0,
        });
        assert!(line.lo >= 380.0 && line.lo < 400.0, "{line:?}");
        let near = value_axis(&ValueSpec {
            data: [10.0, 40.0],
            zero: Zero::Soft,
            min: None,
            max: None,
            length: 300.0,
            horizontal: false,
            format: &f,
            font: &font(),
            size: 12.0,
        });
        assert_eq!(near.lo, 0.0, "room that would reach zero reaches it");
        let bars = value_axis(&ValueSpec {
            data: [400.0, 430.0],
            zero: Zero::Include,
            min: None,
            max: None,
            length: 300.0,
            horizontal: false,
            format: &f,
            font: &font(),
            size: 12.0,
        });
        assert_eq!(bars.lo, 0.0);
        assert!(bars.hi >= 430.0);
        assert_eq!(bars.ticks.first(), Some(&0.0));
        assert_eq!(bars.ticks.last(), Some(&bars.hi), "axes end on labelled ticks");
    }

    #[test]
    fn tick_count_follows_axis_length() {
        let f = Format::default();
        let short = value_axis(&ValueSpec {
            data: [0.0, 100.0],
            zero: Zero::Include,
            min: None,
            max: None,
            length: 120.0,
            horizontal: false,
            format: &f,
            font: &font(),
            size: 12.0,
        });
        let tall = value_axis(&ValueSpec {
            data: [0.0, 100.0],
            zero: Zero::Include,
            min: None,
            max: None,
            length: 520.0,
            horizontal: false,
            format: &f,
            font: &font(),
            size: 12.0,
        });
        assert!(short.ticks.len() < tall.ticks.len());
        assert!(tall.ticks.len() <= 12);
    }

    #[test]
    fn authored_bounds_are_the_axis_ends() {
        let f = Format::default();
        let axis = value_axis(&ValueSpec {
            data: [3.0, 7.0],
            zero: Zero::Include,
            min: Some(2.0),
            max: Some(8.0),
            length: 300.0,
            horizontal: false,
            format: &f,
            font: &font(),
            size: 12.0,
        });
        assert_eq!((axis.lo, axis.hi), (2.0, 8.0));
    }

    #[test]
    fn calendar_labels_carry_the_year_where_it_changes() {
        let months: Vec<i64> = [(2025, 11), (2025, 12), (2026, 1), (2026, 2)]
            .iter()
            .map(|&(y, m)| time::ms_from_civil(y, m, 1))
            .collect();
        let labels = calendar_labels(
            &months,
            Interval {
                unit: Unit::Month,
                step: 1,
            },
            Locale::EnUs,
        );
        assert_eq!(labels, ["Nov 2025", "Dec", "Jan 2026", "Feb"]);
        let days: Vec<i64> = [(2026, 1, 5), (2026, 1, 12)]
            .iter()
            .map(|&(y, m, d)| time::ms_from_civil(y, m, d))
            .collect();
        assert_eq!(
            calendar_labels(
                &days,
                Interval {
                    unit: Unit::Week,
                    step: 1
                },
                Locale::EnUs
            ),
            ["Jan 5, 2026", "Jan 12"]
        );
        assert_eq!(
            calendar_labels(
                &days,
                Interval {
                    unit: Unit::Week,
                    step: 1
                },
                Locale::CsCz
            ),
            ["5. led 2026", "12. led"]
        );
    }

    #[test]
    fn calendar_ticks_never_subdivide_below_the_data() {
        let from = time::ms_from_civil(2020, 1, 1);
        let to = time::ms_from_civil(2020, 12, 1);
        let monthly = 30 * time::MS_DAY;
        let axis = calendar_axis(from, to, monthly, Precision::Month, 1400.0, &font(), 12.0, Locale::EnUs);
        assert_eq!(axis.interval.unit, Unit::Month);
        let narrow = calendar_axis(from, to, monthly, Precision::Month, 300.0, &font(), 12.0, Locale::EnUs);
        assert!(narrow.ticks.len() <= 4, "{narrow:?}");
        let again = calendar_axis(from, to, monthly, Precision::Month, 300.0, &font(), 12.0, Locale::EnUs);
        assert_eq!(narrow, again, "same data and width, same ticks");
    }

    #[test]
    fn a_week_of_days_on_a_phone_keeps_its_first_and_last_day() {
        let from = time::ms_from_civil(2025, 3, 1);
        let to = time::ms_from_civil(2025, 3, 7);
        let axis = calendar_axis(
            from,
            to,
            time::MS_DAY,
            Precision::Day,
            290.0,
            &font(),
            12.0,
            Locale::EnUs,
        );
        assert_eq!(axis.labels, ["Mar 1, 2025", "Mar 3", "Mar 5", "Mar 7"], "{axis:?}");
    }

    #[test]
    fn period_categories_print_in_reader_form() {
        let names: Vec<String> = ["2025-08", "2025-09", "2025-12", "2026-01"].map(String::from).to_vec();
        let o = ordered(&names, Locale::EnUs).unwrap();
        assert_eq!(o.text, ["Aug 2025", "Sep", "Dec", "Jan 2026"]);
        let q: Vec<String> = ["2025-Q3", "2025-Q4", "2026-Q1"].map(String::from).to_vec();
        assert_eq!(ordered(&q, Locale::EnUs).unwrap().text, ["Q3 2025", "Q4", "Q1 2026"]);
    }

    #[test]
    fn category_names_wrap_then_thin_and_are_never_cut() {
        let f = font();
        let names: Vec<String> = ["North-east region", "South-west region"].map(String::from).to_vec();
        match category_layout(&names, None, 110.0, &f, 12.0) {
            CategoryLayout::Wrapped(lines) => assert_eq!(lines[0].len(), 2),
            other => panic!("{other:?}"),
        }
        // Single words that fit two bands but not one take two alternating rows.
        let words: Vec<String> = ["Sleek Dark", "Air", "Scientific", "Brutalist", "Hand-Drawn"]
            .map(String::from)
            .to_vec();
        assert!(matches!(
            category_layout(&words, None, 67.0, &f, 12.0),
            CategoryLayout::Unfit { .. }
        ));
        match category_layout_with(&words, None, 67.0, &f, 12.0, true) {
            CategoryLayout::Wrapped(lines) => {
                assert_eq!(lines[0], ["Sleek Dark"]);
                assert_eq!(lines[1], ["", "Air"]);
                assert_eq!(lines[3], ["", "Brutalist"]);
            }
            other => panic!("{other:?}"),
        }
        let years: Vec<String> = (2000..2030).map(|y| y.to_string()).collect();
        let o = ordered(&years, Locale::EnUs);
        match category_layout(&years, o.as_ref(), 12.0, &f, 12.0) {
            CategoryLayout::Thinned { every, .. } => assert!(every >= 5),
            other => panic!("{other:?}"),
        }
        // Decades step by ten, so every one of them falls on "every 5" of their numbers: the
        // thinning that counts is the first one that leaves names out.
        let decades: Vec<String> = (0..15).map(|i| format!("{}s", 1880 + 10 * i)).collect();
        let o = ordered(&decades, Locale::EnUs);
        match category_layout(&decades, o.as_ref(), 22.0, &f, 12.0) {
            CategoryLayout::Thinned { every, lines } => {
                let printed: Vec<usize> = (0..15).filter(|&i| !lines[i].is_empty()).collect();
                assert_eq!(printed, [0, 2, 4, 6, 8, 10, 12, 14]);
                assert_eq!((every, nth_printed(&lines).as_str()), (20, "2nd"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_short_plot_still_gets_steps_to_read_between() {
        let f = Format::default();
        let face = font();
        let spec = |length| ValueSpec {
            data: [0.0, 63.0],
            zero: Zero::Include,
            min: None,
            max: None,
            length,
            horizontal: false,
            format: &f,
            font: &face,
            size: 12.0,
        };
        let short = value_axis(&spec(180.0));
        assert_eq!((short.lo, short.hi, short.step), (0.0, 70.0, 10.0), "{short:?}");
        let tall = value_axis(&spec(520.0));
        assert!(tall.ticks[1] - tall.ticks[0] < short.ticks[1] - short.ticks[0] + 1e-9 || tall.ticks.len() >= 7);
    }

    #[test]
    fn room_above_the_largest_value_keeps_a_label_inside_the_axis() {
        let f = Format::default();
        let spec = ValueSpec {
            data: [0.0, 120.0],
            zero: Zero::Include,
            min: None,
            max: None,
            length: 300.0,
            horizontal: false,
            format: &f,
            font: &font(),
            size: 12.0,
        };
        let bare = value_axis(&spec);
        let roomy = value_axis_room(&spec, 22.0);
        assert!(roomy.hi > bare.hi - 1e-9);
        let per_px = (roomy.hi - roomy.lo) / 300.0;
        assert!(roomy.hi - 120.0 >= 22.0 * per_px - 1e-6, "{bare:?} {roomy:?}");
    }

    #[test]
    fn a_thinned_axis_names_its_step_as_an_ordinal() {
        // Every `apart`-th of 40 names printed: the finding says "every 2nd", never "every 2th".
        let every = |apart: usize| {
            let lines: Vec<Vec<String>> = (0..40)
                .map(|i| if i % apart == 0 { vec![i.to_string()] } else { vec![] })
                .collect();
            nth_printed(&lines)
        };
        for (apart, ordinal) in [
            (2, "2nd"),
            (3, "3rd"),
            (4, "4th"),
            (5, "5th"),
            (11, "11th"),
            (12, "12th"),
            (13, "13th"),
            (21, "21st"),
            (22, "22nd"),
            (23, "23rd"),
        ] {
            assert_eq!(every(apart), ordinal);
        }
    }
}
