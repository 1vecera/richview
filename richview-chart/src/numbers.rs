//! Numbers as readers see them: the three-digit rule (FEA-CH-37, TYP-23).
//!
//! Every number is rounded to a precision in significant digits, three by default; whole numbers
//! below 10,000 print whole, larger ones compact with a scale word (`167 thousand`, `1.23
//! million`) or, on axes, K, M and B (FEA-CH-22). Percentages multiply by 100, years print as
//! plain integers, and units follow one placement rule. Separators follow the report language.

use serde::Serialize;

/// Default significant digits (FEA-CH-37).
pub const DEFAULT_DIGITS: u8 = 3;
/// Largest precision an author may set.
pub const MAX_DIGITS: u8 = 15;

/// Number separators per supported locale (TYP-08).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum Locale {
    #[default]
    EnUs,
    EnGb,
    CsCz,
    DeDe,
    JaJp,
}

impl Locale {
    /// The locale for a BCP 47 tag; unsupported tags format as English.
    pub fn from_tag(tag: &str) -> Locale {
        let lower = tag.to_ascii_lowercase();
        match lower.as_str() {
            "en-gb" => Locale::EnGb,
            t if t.starts_with("cs") => Locale::CsCz,
            t if t.starts_with("de") => Locale::DeDe,
            t if t.starts_with("ja") => Locale::JaJp,
            _ => Locale::EnUs,
        }
    }
    pub fn supported(tag: &str) -> bool {
        matches!(
            tag.to_ascii_lowercase().as_str(),
            "en" | "en-us" | "en-gb" | "cs" | "cs-cz" | "de" | "de-de" | "ja" | "ja-jp"
        )
    }
    fn group(self) -> &'static str {
        match self {
            Locale::EnUs | Locale::EnGb | Locale::JaJp => ",",
            Locale::CsCz => "\u{a0}",
            Locale::DeDe => ".",
        }
    }
    fn decimal(self) -> char {
        match self {
            Locale::EnUs | Locale::EnGb | Locale::JaJp => '.',
            Locale::CsCz | Locale::DeDe => ',',
        }
    }
}

/// A value-format name (TYP-23).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// `number` below 10,000, `number_compact` from 10,000.
    #[default]
    Auto,
    /// Grouped, never scaled.
    Number,
    /// Scale words; K, M and B on axes.
    NumberCompact,
    Percent,
    Year,
}

impl Kind {
    pub fn parse(name: &str) -> Option<Kind> {
        Some(match name {
            "number" => Kind::Number,
            "number_compact" => Kind::NumberCompact,
            "percent" => Kind::Percent,
            "year" => Kind::Year,
            _ => return None,
        })
    }
}

/// How one channel's numbers print.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Format {
    pub kind: Kind,
    pub unit: Option<String>,
    pub digits: u8,
    pub locale: Locale,
}

impl Default for Format {
    fn default() -> Self {
        Self {
            kind: Kind::Auto,
            unit: None,
            digits: DEFAULT_DIGITS,
            locale: Locale::EnUs,
        }
    }
}

/// The scale words of values and readouts, per locale, after the number (FEA-CH-37 as
/// amended in ASSUMED.md): the Czech and German abbreviations, English words elsewhere.
fn scale_words(locale: Locale) -> [&'static str; 5] {
    match locale {
        Locale::EnUs | Locale::EnGb | Locale::JaJp => ["", "thousand", "million", "billion", "trillion"],
        Locale::CsCz => ["", "tis.", "mil.", "mld.", "bil."],
        Locale::DeDe => ["", "Tsd.", "Mio.", "Mrd.", "Bio."],
    }
}
const SCALE_SUFFIX: [&str; 5] = ["", "K", "M", "B", "T"];

/// An axis's compact suffixes: K and M in every locale, then B and T, except where the
/// language's own billion is 10^12. Czech and German ticks write their words for 10^9 and
/// 10^12, so `3B Kč` is never read a thousandfold off (FEA-CH-22 as amended in ASSUMED.md).
fn axis_suffixes(locale: Locale) -> [&'static str; 5] {
    match locale {
        Locale::CsCz => ["", "K", "M", " mld.", " bil."],
        Locale::DeDe => ["", "K", "M", " Mrd.", " Bio."],
        Locale::EnUs | Locale::EnGb | Locale::JaJp => SCALE_SUFFIX,
    }
}

/// `value` rounded to `digits` significant digits, half away from zero, on its shortest decimal
/// text: `1234.5` at four digits is `1235`, as the author wrote it, not the binary tie `1234`.
pub fn round_sig(value: f64, digits: u8) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let digits = digits.clamp(1, 17) as usize;
    // Shortest round-trip text, e.g. "1.2345e3".
    let text = format!("{:e}", value.abs());
    let (mantissa, exp) = text.split_once('e').unwrap_or((&text, "0"));
    let mut exp: i32 = exp.parse().unwrap_or(0);
    let original_exp = exp;
    let mut ds: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
    if ds.len() > digits {
        let round_up = ds[digits] >= 5;
        ds.truncate(digits);
        if round_up {
            let mut i = digits;
            loop {
                if i == 0 {
                    ds.insert(0, 1);
                    ds.truncate(digits);
                    exp += 1;
                    break;
                }
                i -= 1;
                if ds[i] == 9 {
                    ds[i] = 0;
                } else {
                    ds[i] += 1;
                    break;
                }
            }
        }
    }
    let digits_text: String = ds.iter().map(|d| char::from(b'0' + d)).collect();
    let sign = if value < 0.0 { "-" } else { "" };
    let rounded = format!("{sign}{}.{}e{exp}", &digits_text[..1], &digits_text[1..]);
    let parsed = rounded
        .trim_end_matches(".e0")
        .parse()
        .or_else(|_| rounded.replace(".e", "e").parse())
        .unwrap_or(value);
    if f64::is_finite(parsed) {
        parsed
    } else {
        // Rounding f64::MAX upward can overflow. Keep the largest finite decimal at the
        // requested precision rather than an infinity or the finer exact source value.
        let truncated: String = mantissa
            .bytes()
            .filter(u8::is_ascii_digit)
            .take(digits)
            .map(char::from)
            .collect();
        format!("{sign}{}.{}e{original_exp}", &truncated[..1], &truncated[1..])
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .unwrap_or(value)
    }
}

/// Significant digits a whole number below 10,000 keeps: it prints whole (FEA-CH-22).
pub fn effective_digits(value: f64, digits: u8) -> u8 {
    let magnitude = value.abs();
    if (1000.0..10000.0).contains(&magnitude) {
        digits.max(4)
    } else {
        digits
    }
}

/// Round `value` the way readers see it under `format`: what the payload may carry (TYP-36).
pub fn shown(value: f64, format: &Format) -> f64 {
    match format.kind {
        Kind::Year => value.round(),
        Kind::Percent => round_sig(value, format.digits),
        Kind::Number => round_sig(value, effective_digits(value, format.digits)),
        Kind::Auto | Kind::NumberCompact => {
            let rounded = round_sig(value, effective_digits(value, format.digits));
            if rounded.abs() >= 10_000.0 || (format.kind == Kind::NumberCompact && rounded.abs() >= 1000.0) {
                let (power, mantissa) = scale_of(rounded, format.digits);
                let compact = round_sig(mantissa * 1000f64.powi(power as i32), format.digits);
                if compact.is_finite() { compact } else { rounded }
            } else {
                rounded
            }
        }
    }
}

/// Decimal digits needed to print `value` (already rounded) without losing digits.
fn decimals_of(value: f64) -> usize {
    if value == 0.0 || !value.is_finite() {
        return 0;
    }
    let text = format!("{value}");
    if let Some((_, exp)) = text.split_once('e') {
        let exp: i32 = exp.parse().unwrap_or(0);
        let mantissa_decimals = text
            .split_once('.')
            .map_or(0, |(_, rest)| rest.split('e').next().unwrap_or("").len());
        return (mantissa_decimals as i32 - exp).max(0) as usize;
    }
    text.split_once('.').map_or(0, |(_, d)| d.len())
}

/// Format a coordinate like an axis label, with precision from the author-rounded value.
pub(crate) fn readout_coordinate(value: f64, format: &Format) -> String {
    let value = shown(value, format);
    let magnitude = value.abs();
    let compact =
        format.kind == Kind::Auto && magnitude >= 10_000.0 || format.kind == Kind::NumberCompact && magnitude >= 1000.0;
    let scale = if format.kind == Kind::Percent {
        0.01
    } else if compact {
        1000f64.powi(((magnitude.log10() / 3.0).floor() as i32).min(SCALE_SUFFIX.len() as i32 - 1))
    } else {
        1.0
    };
    let digits = if format.kind == Kind::Percent {
        format.digits
    } else {
        effective_digits(value, format.digits)
    };
    let decimals = decimals_of(round_sig(value / scale, digits)).min(12);
    let step = scale * 10f64.powi(-(decimals as i32));
    axis_labels(&[value], step, format).remove(0)
}

/// Plain decimal text of `value` with `decimals` decimals, grouped per locale.
pub fn grouped(value: f64, decimals: usize, locale: Locale, group: bool) -> String {
    let text = format!("{:.*}", decimals, value.abs());
    let (int, frac) = match text.split_once('.') {
        Some((i, f)) => (i.to_owned(), Some(f.to_owned())),
        None => (text.clone(), None),
    };
    let mut out = String::new();
    if value < 0.0 && text.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.push('\u{2212}');
    }
    if group && int.len() > 3 {
        let len = int.len();
        for (i, ch) in int.chars().enumerate() {
            if i > 0 && (len - i) % 3 == 0 {
                out.push_str(locale.group());
            }
            out.push(ch);
        }
    } else {
        out.push_str(&int);
    }
    if let Some(frac) = frac {
        out.push(locale.decimal());
        out.push_str(&frac);
    }
    out
}

/// Superscript digits for exponents: `10⁻⁶`.
fn superscript(exp: i32) -> String {
    let digits = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    let mut out = String::new();
    if exp < 0 {
        out.push('⁻');
    }
    for ch in exp.unsigned_abs().to_string().chars() {
        out.push(digits[ch.to_digit(10).unwrap_or(0) as usize]);
    }
    out
}

/// Scientific notation `1.2 × 10⁻⁶` with `digits` significant digits (FEA-CH-23).
pub fn scientific(value: f64, digits: u8, locale: Locale) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let text = format!("{:.*e}", (digits.max(1) - 1) as usize, value);
    let (mantissa, exp) = text.split_once('e').unwrap_or((&text, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let mantissa: f64 = mantissa.parse().unwrap_or(value);
    let m = grouped(mantissa, decimals_of(mantissa), locale, false);
    if exp == 0 {
        m
    } else {
        format!("{m} × 10{}", superscript(exp))
    }
}

/// Attach a unit by the T-VALUE-FORMAT rule: currency symbols lead, `%` and degree signs
/// attach, every other unit follows after a space.
pub fn with_unit(number: &str, unit: Option<&str>) -> String {
    let Some(unit) = unit.map(str::trim).filter(|u| !u.is_empty()) else {
        return number.to_owned();
    };
    if is_currency(unit) {
        // A minus sign goes before the symbol: −$12.
        return match number.strip_prefix('\u{2212}') {
            Some(rest) => format!("\u{2212}{unit}{rest}"),
            None => format!("{unit}{number}"),
        };
    }
    if unit.starts_with('%') || unit.starts_with('°') || unit.starts_with('′') || unit.starts_with('″') {
        return format!("{number}{unit}");
    }
    format!("{number}\u{a0}{unit}")
}

/// A unit that is or ends in a currency symbol leads the value (TYP-23).
pub fn is_currency(unit: &str) -> bool {
    ["$", "€", "£", "¥"].iter().any(|s| unit.ends_with(s))
}

/// `v` as an automatic value prints it on its mark, when that differs from its readout text:
/// the readout's format at the label's `digits`. The author may give readouts more digits than
/// labels (FEA-CH-37); a printed value never shows more than the labels'. `None` when the
/// readout has no more digits than the label, so the printed text is the readout's.
pub fn printed(v: f64, readout: &Format, digits: u8) -> Option<String> {
    (digits < readout.digits).then(|| {
        value(
            v,
            &Format {
                digits,
                ..readout.clone()
            },
        )
    })
}

/// `v` as its mark prints it: [`printed`]'s text, or the readout's when they agree.
pub fn label(v: f64, readout: &Format, digits: u8) -> String {
    printed(v, readout, digits).unwrap_or_else(|| value(v, readout))
}

/// One value as readouts, tables, metrics and CSV-adjacent text show it, with scale words.
pub fn value(value: f64, format: &Format) -> String {
    if !value.is_finite() {
        return "n/a".into();
    }
    let value = shown(value, format);
    let locale = format.locale;
    match format.kind {
        Kind::Year => grouped(value.round(), 0, locale, false),
        Kind::Percent => {
            let pct = round_sig(value * 100.0, format.digits);
            if pct != 0.0 && pct.abs() < 1e-8 {
                return format!("{}%", scientific(pct, format.digits, locale));
            }
            format!("{}%", grouped(pct, decimals_of(pct), locale, true))
        }
        Kind::Number => plain_or_scientific(value, format),
        Kind::Auto | Kind::NumberCompact => {
            if value.abs() >= 10_000.0 || (format.kind == Kind::NumberCompact && value.abs() >= 1000.0) {
                compact_word(value, format)
            } else {
                plain_or_scientific(value, format)
            }
        }
    }
}

fn plain_or_scientific(v: f64, format: &Format) -> String {
    let unit = format.unit.as_deref();
    if v != 0.0 && (v.abs() < 1e-8 || v.abs() >= 1e15) {
        return with_unit(&scientific(v, format.digits, format.locale), unit);
    }
    with_unit(&grouped(v, decimals_of(v), format.locale, true), unit)
}

fn compact_word(v: f64, format: &Format) -> String {
    let words = scale_words(format.locale);
    let (k, mantissa) = scale_of(v, format.digits);
    if k >= words.len() {
        return with_unit(&scientific(v, format.digits, format.locale), format.unit.as_deref());
    }
    let number = grouped(mantissa, decimals_of(mantissa), format.locale, true);
    let words = format!("{number} {}", words[k]);
    with_unit(&words, format.unit.as_deref())
}

/// The thousand-power `k` and rounded mantissa of `v`: 167,500 at three digits → (1, 168).
fn scale_of(v: f64, digits: u8) -> (usize, f64) {
    if !v.is_finite() {
        return (0, v);
    }
    let mut k = ((v.abs().log10() / 3.0).floor().max(0.0)) as usize;
    let mut mantissa = round_sig(v / 1000f64.powi(k as i32), digits);
    if mantissa.abs() >= 1000.0 {
        k += 1;
        mantissa = round_sig(v / 1000f64.powi(k as i32), digits);
    }
    (k, mantissa)
}

/// Labels for one axis's ticks: one magnitude and one precision for the whole axis
/// (FEA-CH-22, FEA-CH-23). `step` is the tick spacing.
pub fn axis_labels(ticks: &[f64], step: f64, format: &Format) -> Vec<String> {
    let locale = format.locale;
    let unit = format.unit.as_deref();
    let largest = ticks.iter().fold(0.0f64, |m, t| m.max(t.abs()));
    match format.kind {
        Kind::Year => ticks.iter().map(|t| grouped(t.round(), 0, locale, false)).collect(),
        Kind::Percent => {
            let pct_step = step * 100.0;
            let decimals = step_decimals(pct_step);
            let labels: Vec<String> = ticks
                .iter()
                .map(|t| {
                    let v = t * 100.0;
                    if v.abs() < pct_step * 1e-6 {
                        "0%".into()
                    } else {
                        format!("{}%", grouped(v, decimals, locale, true))
                    }
                })
                .collect();
            if needs_scientific(&labels, ticks, decimals) {
                return scientific_axis(ticks, format);
            }
            labels
        }
        _ => {
            let compact = format.kind == Kind::NumberCompact && largest >= 1000.0
                || format.kind == Kind::Auto && largest >= 10_000.0;
            let (k, scale) = if compact {
                let k = ((largest.log10() / 3.0).floor() as usize).min(SCALE_SUFFIX.len() - 1);
                (k, 1000f64.powi(k as i32))
            } else {
                (0, 1.0)
            };
            let decimals = step_decimals(step / scale);
            let (magnitude_unit, suffix) = scaled_unit(unit, k, locale);
            // The 12-character limit measures the number; a unit's length never forces
            // scientific notation (FEA-CH-23).
            let numbers: Vec<String> = ticks
                .iter()
                .map(|t| {
                    if t.abs() < step * 1e-9 {
                        return "0".into();
                    }
                    format!("{}{suffix}", grouped(t / scale, decimals, locale, true))
                })
                .collect();
            if needs_scientific(&numbers, ticks, decimals) {
                return scientific_axis(ticks, format);
            }
            numbers
                .iter()
                .zip(ticks)
                .map(|(n, t)| {
                    if t.abs() < step * 1e-9 {
                        n.clone()
                    } else {
                        with_unit(n, magnitude_unit.as_deref())
                    }
                })
                .collect()
        }
    }
}

/// Magnitude units scale once instead of taking a compact suffix (FEA-CH-23, TYP-23): `K`
/// reads `M`, `M` reads `B`, `KBD` reads `MBD`. Other units keep the K, M, B suffix.
fn scaled_unit(unit: Option<&str>, k: usize, locale: Locale) -> (Option<String>, &'static str) {
    let suffixes = axis_suffixes(locale);
    let Some(unit) = unit else {
        return (None, suffixes[k]);
    };
    if k == 0 {
        return (Some(unit.to_owned()), "");
    }
    let ladder: &[&str] = match unit {
        "k" | "K" => &["K", "M", "B", "T"],
        "M" => &["M", "B", "T"],
        "KBD" => &["KBD", "MBD", "BBD"],
        _ => return (Some(unit.to_owned()), suffixes[k]),
    };
    match ladder.get(k) {
        Some(next) => (Some((*next).to_owned()), ""),
        None => (Some(unit.to_owned()), suffixes[k]),
    }
}

/// Decimals that print every multiple of `step` exactly: 0.25 → 2, 5 → 0.
pub fn step_decimals(step: f64) -> usize {
    if !(step.is_finite() && step > 0.0) {
        return 0;
    }
    for decimals in 0..=12 {
        let scaled = step * 10f64.powi(decimals);
        if (scaled - scaled.round()).abs() < 1e-6 * scaled.max(1.0) {
            return decimals as usize;
        }
    }
    12
}

/// An axis switches to scientific notation when a label exceeds 12 characters, a nonzero tick
/// prints as zero or needs more than eight decimals (FEA-CH-23).
fn needs_scientific(labels: &[String], ticks: &[f64], decimals: usize) -> bool {
    decimals > 8
        || labels.iter().any(|l| l.chars().count() > 12)
        || ticks
            .iter()
            .zip(labels)
            .any(|(t, l)| *t != 0.0 && l.trim_matches(|c: char| !c.is_ascii_digit()).chars().all(|c| c == '0'))
}

fn scientific_axis(ticks: &[f64], format: &Format) -> Vec<String> {
    let scale = if format.kind == Kind::Percent { 100.0 } else { 1.0 };
    let suffix = if format.kind == Kind::Percent { "%" } else { "" };
    ticks
        .iter()
        .map(|t| {
            if *t == 0.0 {
                return format!("0{suffix}");
            }
            let text = scientific(t * scale, format.digits, format.locale);
            if suffix.is_empty() {
                with_unit(&text, format.unit.as_deref())
            } else {
                format!("{text}{suffix}")
            }
        })
        .collect()
}

/// Whether every value is an integer between 1000 and 2999: a year axis (FEA-CH-21).
pub fn looks_like_years(values: &[f64]) -> bool {
    !values.is_empty() && values.iter().all(|v| v.fract() == 0.0 && (1000.0..=2999.0).contains(v))
}

/// A number for CSV: plain digits, no grouping, rounded to the readout precision (FEA-CH-48).
pub fn csv_number(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    let text = format!("{value}");
    if text.contains('e') {
        // Expand exponent notation into plain digits.
        let decimals = decimals_of(value);
        return format!("{value:.decimals$}");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(kind: Kind, unit: Option<&str>, digits: u8) -> Format {
        Format {
            kind,
            unit: unit.map(str::to_owned),
            digits,
            locale: Locale::EnUs,
        }
    }

    #[test]
    fn three_digit_rule_with_scale_words() {
        let auto = Format::default();
        assert_eq!(value(1_234_567.0, &auto), "1.23 million");
        assert_eq!(value(167_000.0, &auto), "167 thousand");
        assert_eq!(value(167_500.0, &fmt(Kind::Auto, None, 4)), "167.5 thousand");
        assert_eq!(value(9_876.0, &auto), "9,876", "whole numbers below 10,000 print whole");
        assert_eq!(value(422.8, &auto), "423");
        assert_eq!(value(0.0012345, &auto), "0.00123");
        assert_eq!(value(0.0, &auto), "0");
        assert_eq!(value(-1500.0, &auto), "\u{2212}1,500");
        assert_eq!(
            value(999_999.0, &auto),
            "1 million",
            "a mantissa that rounds to 1000 moves up"
        );
    }

    #[test]
    fn readers_never_get_more_digits_than_the_author_chose() {
        assert_eq!(shown(422.87, &Format::default()), 423.0);
        assert_eq!(shown(422.87, &fmt(Kind::Auto, None, 5)), 422.87);
        assert_eq!(shown(0.123456, &fmt(Kind::Percent, None, 3)), 0.123);
        assert_eq!(shown(1734.0, &fmt(Kind::Year, None, 3)), 1734.0);
        assert_eq!(shown(5432.1, &Format::default()), 5432.0);
    }

    #[test]
    fn units_follow_one_placement_rule() {
        assert_eq!(value(12.0, &fmt(Kind::Auto, Some("users"), 3)), "12\u{a0}users");
        let per_person = fmt(Kind::Auto, Some("units/person"), 3);
        assert_eq!(
            axis_labels(&[0.0, 0.05, 0.1], 0.05, &per_person),
            ["0", "0.05\u{a0}units/person", "0.10\u{a0}units/person"],
            "a long unit keeps plain numbers"
        );
        assert_eq!(value(1_230_000.0, &fmt(Kind::Number, Some("$"), 3)), "$1,230,000");
        assert_eq!(value(1_230_000.0, &fmt(Kind::Auto, Some("$"), 3)), "$1.23 million");
        assert_eq!(value(21.5, &fmt(Kind::Auto, Some("°C"), 3)), "21.5°C");
        assert_eq!(value(-12.0, &fmt(Kind::Auto, Some("€"), 3)), "\u{2212}€12");
        let cz = Format {
            kind: Kind::Number,
            unit: Some("Kč".into()),
            digits: 4,
            locale: Locale::CsCz,
        };
        assert_eq!(value(1234.0, &cz), "1\u{a0}234\u{a0}Kč");
    }

    #[test]
    fn scale_words_follow_the_locale() {
        let at = |locale: Locale, unit: Option<&str>, v: f64| {
            value(
                v,
                &Format {
                    kind: Kind::NumberCompact,
                    unit: unit.map(Into::into),
                    digits: 4,
                    locale,
                },
            )
        };
        assert_eq!(at(Locale::CsCz, Some("Kč"), 280_512_345.0), "280,5 mil.\u{a0}Kč");
        assert_eq!(
            [12_345.0, 4.5e6, 7.25e9, 1.5e12].map(|v| at(Locale::CsCz, None, v)),
            ["12,35 tis.", "4,5 mil.", "7,25 mld.", "1,5 bil."]
        );
        assert_eq!(
            [12_345.0, 4.5e6, 7.25e9, 1.5e12].map(|v| at(Locale::DeDe, None, v)),
            ["12,35 Tsd.", "4,5 Mio.", "7,25 Mrd.", "1,5 Bio."]
        );
        // Axis ticks keep K and M everywhere; Czech and German write their own words for 10^9
        // and 10^12, where their billion is 10^12.
        let axis = |locale: Locale, unit: Option<&str>, ticks: [f64; 3]| {
            let f = Format {
                kind: Kind::NumberCompact,
                unit: unit.map(Into::into),
                digits: 3,
                locale,
            };
            axis_labels(&ticks, ticks[1] - ticks[0], &f)
        };
        let billions = [0.0, 1.5e9, 3e9];
        let trillions = [0.0, 1.5e12, 3e12];
        assert_eq!(
            axis(Locale::CsCz, Some("Kč"), billions),
            ["0", "1,5 mld.\u{a0}Kč", "3,0 mld.\u{a0}Kč"]
        );
        assert_eq!(axis(Locale::CsCz, None, trillions), ["0", "1,5 bil.", "3,0 bil."]);
        assert_eq!(axis(Locale::DeDe, None, billions), ["0", "1,5 Mrd.", "3,0 Mrd."]);
        assert_eq!(axis(Locale::DeDe, None, trillions), ["0", "1,5 Bio.", "3,0 Bio."]);
        assert_eq!(axis(Locale::EnGb, None, billions), ["0", "1.5B", "3.0B"]);
        assert_eq!(axis(Locale::EnUs, None, trillions), ["0", "1.5T", "3.0T"]);
        assert_eq!(axis(Locale::JaJp, None, billions), ["0", "1.5B", "3.0B"]);
        assert_eq!(axis(Locale::CsCz, None, [0.0, 1.5e6, 3e6]), ["0", "1,5M", "3,0M"]);
        // Japanese keeps the English words for now; myriads are only proposed (ASSUMED.md).
        assert_eq!(at(Locale::JaJp, None, 280_512_345.0), "280.5 million");
        assert_eq!(at(Locale::EnGb, None, 4.5e6), "4.5 million");
    }

    #[test]
    fn percentages_scale_once() {
        assert_eq!(value(0.1234, &fmt(Kind::Percent, None, 3)), "12.3%");
        assert_eq!(
            axis_labels(&[0.0, 0.25, 0.5], 0.25, &fmt(Kind::Percent, None, 3)),
            ["0%", "25%", "50%"]
        );
        assert_eq!(
            axis_labels(&[0.0, 0.005, 0.01], 0.005, &fmt(Kind::Percent, None, 3)),
            ["0%", "0.5%", "1.0%"],
            "fractional ticks keep one shared precision"
        );
        assert_eq!(
            axis_labels(&[-0.5, 0.0, 0.5], 0.5, &fmt(Kind::Percent, None, 3)),
            ["−50%", "0%", "50%"]
        );
    }

    #[test]
    fn axes_compact_from_ten_thousand_and_share_one_precision() {
        let auto = Format::default();
        assert_eq!(
            axis_labels(&[0.0, 500.0, 1000.0, 1500.0], 500.0, &auto),
            ["0", "500", "1,000", "1,500"]
        );
        assert_eq!(
            axis_labels(&[0.0, 12_500.0, 25_000.0], 12_500.0, &auto),
            ["0", "12.5K", "25.0K"]
        );
        assert_eq!(axis_labels(&[0.0, 0.5, 1.0], 0.5, &auto), ["0", "0.5", "1.0"]);
        assert_eq!(
            axis_labels(&[1700.0, 1720.0], 20.0, &fmt(Kind::Year, None, 3)),
            ["1700", "1720"]
        );
        let kbd = fmt(Kind::Auto, Some("KBD"), 3);
        assert_eq!(
            axis_labels(&[0.0, 500.0, 1000.0], 500.0, &kbd),
            ["0", "500\u{a0}KBD", "1,000\u{a0}KBD"]
        );
        assert_eq!(
            axis_labels(&[0.0, 10_000.0, 20_000.0], 10_000.0, &kbd),
            ["0", "10\u{a0}MBD", "20\u{a0}MBD"]
        );
        assert_eq!(
            axis_labels(&[0.0, 10_000.0, 20_000.0], 10_000.0, &fmt(Kind::Auto, Some("$"), 3)),
            ["0", "$10K", "$20K"]
        );
    }

    #[test]
    fn tiny_ticks_switch_to_scientific_notation() {
        let labels = axis_labels(&[0.0, 1e-9, 2e-9], 1e-9, &Format::default());
        assert_eq!(labels, ["0", "1 × 10⁻⁹", "2 × 10⁻⁹"]);
        assert_eq!(scientific(0.0000012, 2, Locale::EnUs), "1.2 × 10⁻⁶");
    }

    #[test]
    fn separators_follow_the_language() {
        let de = Format {
            locale: Locale::DeDe,
            ..Format::default()
        };
        assert_eq!(value(1234.5, &de), "1.235");
        assert_eq!(value(0.5, &de), "0,5");
        for (locale, whole) in [
            (Locale::DeDe, ["0%", "2.500%", "5.000%"]),
            (Locale::CsCz, ["0%", "2\u{a0}500%", "5\u{a0}000%"]),
        ] {
            let percent = Format {
                kind: Kind::Percent,
                locale,
                ..Format::default()
            };
            assert_eq!(axis_labels(&[0.0, 25.0, 50.0], 25.0, &percent), whole);
            assert_eq!(
                axis_labels(&[0.0, 0.005, 0.01], 0.005, &percent),
                ["0%", "0,5%", "1,0%"]
            );
        }
        assert_eq!(Locale::from_tag("cs-CZ"), Locale::CsCz);
        assert!(!Locale::supported("fr"));
    }

    #[test]
    fn years_and_csv() {
        assert!(looks_like_years(&[1700.0, 1720.0, 2024.0]));
        assert!(!looks_like_years(&[1700.0, 1720.5]));
        assert_eq!(csv_number(0.000001), "0.000001");
        assert_eq!(csv_number(1234.5), "1234.5");
    }
}
