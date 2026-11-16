//! Reading resolved rows by channel: missing values, numbers and calendar values (TYP-90,
//! TYP-91).
//!
//! Rows arrive as JSON objects, already projected to the fields the chart references and rounded
//! to the author's precision (TYP-36). One rule covers every source: null, an empty string and
//! the markers `.`, `NA`, `N/A`, `NaN`, `null` and `none` are missing; any other text in a numeric
//! channel is an error naming the row and value.

use crate::block::ChartType;
use crate::time;
use serde_json::Value;

/// One cell of a numeric channel.
#[derive(Debug, Clone, PartialEq)]
pub enum Num {
    Missing,
    Value(f64),
    /// Text that is not a number, such as `1,234` (`chart.non-numeric`).
    Invalid(String),
    /// Nonzero text too small to represent; never becomes zero (FEA-CH-70).
    Underflow(String),
}

/// Whether text is a missing-value marker (TYP-91).
pub fn is_missing_text(text: &str) -> bool {
    let t = text.trim();
    t.is_empty() || [".", "na", "n/a", "nan", "null", "none"].contains(&t.to_ascii_lowercase().as_str())
}

pub fn is_missing(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => is_missing_text(s),
        _ => false,
    }
}

/// The cell of `field` in `row`, `Null` when absent.
pub fn cell<'a>(row: &'a Value, field: &str) -> &'a Value {
    row.get(field).unwrap_or(&Value::Null)
}

/// Parse plain decimal text: digits with an optional sign, point and exponent. Grouped numbers
/// and words are not numbers.
pub fn parse_decimal(text: &str) -> Option<Result<f64, ()>> {
    let t = text.trim();
    let body = t.strip_prefix(['-', '+']).unwrap_or(t);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], Some(&body[at + 1..])),
        None => (body, None),
    };
    let digits_ok = !mantissa.is_empty()
        && mantissa.chars().filter(|c| *c == '.').count() <= 1
        && mantissa.chars().all(|c| c.is_ascii_digit() || c == '.')
        && mantissa.chars().any(|c| c.is_ascii_digit());
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['-', '+']).unwrap_or(e);
        !e.is_empty() && e.chars().all(|c| c.is_ascii_digit())
    });
    if !digits_ok || !exponent_ok {
        return None;
    }
    let value: f64 = t.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 && mantissa.chars().any(|c| ('1'..='9').contains(&c)) {
        return Some(Err(()));
    }
    Some(Ok(value))
}

/// Read a numeric channel.
pub fn num(row: &Value, field: &str) -> Num {
    match cell(row, field) {
        Value::Number(n) => match n.as_f64() {
            Some(v) if v.is_finite() => Num::Value(v),
            _ => Num::Invalid(n.to_string()),
        },
        Value::String(s) if is_missing_text(s) => Num::Missing,
        Value::String(s) => match parse_decimal(s) {
            Some(Ok(v)) => Num::Value(v),
            Some(Err(())) => Num::Underflow(s.clone()),
            None => Num::Invalid(s.clone()),
        },
        Value::Null => Num::Missing,
        Value::Bool(b) => Num::Invalid(b.to_string()),
        other => Num::Invalid(other.to_string()),
    }
}

/// Category text as the source wrote it; `None` when missing.
pub fn text(row: &Value, field: &str) -> Option<String> {
    match cell(row, field) {
        v if is_missing(v) => None,
        Value::String(s) => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        other => Some(other.to_string()),
    }
}

/// How the x channel is scaled (TYP-14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XKind {
    Calendar,
    Numeric,
    Category,
}

/// Line and area use a calendar axis when every `x` is a calendar string, a numeric axis when
/// every `x` is a number or numeric string, and categories otherwise; bar `x` is always
/// categorical and scatter `x` always numeric. One missing or malformed `x` is enough to leave
/// the calendar (TYP-90).
pub fn x_kind(kind: ChartType, field: &str, rows: &[Value]) -> XKind {
    match kind {
        ChartType::Scatter => XKind::Numeric,
        ChartType::Line | ChartType::Area => {
            if rows.is_empty() {
                return XKind::Category;
            }
            if rows
                .iter()
                .all(|r| cell(r, field).as_str().and_then(time::parse_iso).is_some())
            {
                XKind::Calendar
            } else if rows
                .iter()
                .all(|r| matches!(num(r, field), Num::Value(_) | Num::Underflow(_)))
            {
                // Text too small to represent stays on the numeric axis so it is reported as a
                // source error, never read as a category name (FEA-CH-70).
                XKind::Numeric
            } else {
                XKind::Category
            }
        }
        _ => XKind::Category,
    }
}

/// Distinct values of a text channel in first-seen source order (FEA-CH-09).
pub fn distinct(rows: &[Value], field: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    rows.iter()
        .filter_map(|r| text(r, field))
        .filter(|t| seen.insert(t.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_markers_are_gaps_and_other_text_is_an_error() {
        let row = json!({"a": null, "b": " NA ", "c": "12.5", "d": "1,234", "e": 7, "f": "", "g": "1e-400"});
        assert_eq!(num(&row, "a"), Num::Missing);
        assert_eq!(num(&row, "b"), Num::Missing);
        assert_eq!(num(&row, "c"), Num::Value(12.5));
        assert_eq!(num(&row, "d"), Num::Invalid("1,234".into()));
        assert_eq!(num(&row, "e"), Num::Value(7.0));
        assert_eq!(num(&row, "f"), Num::Missing);
        assert_eq!(num(&row, "g"), Num::Underflow("1e-400".into()));
        assert_eq!(num(&row, "absent"), Num::Missing);
        assert_eq!(parse_decimal("inf"), None);
        assert_eq!(parse_decimal("0.0"), Some(Ok(0.0)));
    }

    #[test]
    fn x_scale_follows_todays_rules() {
        let dates = vec![json!({"x": "2020-01"}), json!({"x": "2020-02"})];
        assert_eq!(x_kind(ChartType::Line, "x", &dates), XKind::Calendar);
        let years = vec![json!({"x": "2020"}), json!({"x": "2021"})];
        assert_eq!(
            x_kind(ChartType::Area, "x", &years),
            XKind::Calendar,
            "quoted years are calendar years"
        );
        let numbers = vec![json!({"x": 2020}), json!({"x": "2021"})];
        assert_eq!(x_kind(ChartType::Line, "x", &numbers), XKind::Numeric);
        let tiny = vec![json!({"x": 1}), json!({"x": "1e-400"})];
        assert_eq!(
            x_kind(ChartType::Line, "x", &tiny),
            XKind::Numeric,
            "underflow is an error, not a category"
        );
        let only_tiny = vec![json!({"x": "1e-400"})];
        assert_eq!(x_kind(ChartType::Area, "x", &only_tiny), XKind::Numeric);
        let broken = vec![json!({"x": "2020-01"}), json!({"x": null})];
        assert_eq!(x_kind(ChartType::Line, "x", &broken), XKind::Category);
        assert_eq!(x_kind(ChartType::Bar, "x", &numbers), XKind::Category);
        assert_eq!(
            distinct(&[json!({"c": "b"}), json!({"c": "a"}), json!({"c": "b"})], "c"),
            ["b", "a"]
        );
    }
}
