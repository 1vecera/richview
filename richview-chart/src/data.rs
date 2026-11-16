//! Source number text: the finite and round-trip parsers chart values go through, and an exact
//! decimal order for cells that share a rounded binary64 value.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberError {
    /// Invalid text or a value outside the finite upper range.
    Invalid,
    /// Nonzero decimal text rounded to zero by the numeric parser.
    Underflow,
}

/// Keep each caller's whitespace/missing-value policy, while sharing the finite range.
/// Parsing a nonzero decimal below half the smallest subnormal otherwise returns zero.
pub fn parse_finite_number(text: &str) -> Result<f64, NumberError> {
    let value = text.parse::<f64>().map_err(|_| NumberError::Invalid)?;
    if !value.is_finite() {
        return Err(NumberError::Invalid);
    }
    if value == 0.0
        && text
            .bytes()
            .take_while(|byte| !matches!(byte, b'e' | b'E'))
            .any(|byte| matches!(byte, b'1'..=b'9'))
    {
        return Err(NumberError::Underflow);
    }
    Ok(value)
}

/// Parse a decimal cell only when JSON number serialization retains its decimal value.
/// Keep the source text when this returns `None`; geometry can still use `parse_finite_number`.
pub fn parse_roundtrip_number(text: &str) -> Option<f64> {
    let value = parse_finite_number(text).ok()?;
    // At most 15 bytes implies at most 15 significant digits. Normal binary64 values retain
    // that many decimal digits; subnormal and longer inputs need the exact comparison.
    if value == 0.0 || value.is_normal() && text.len() <= f64::DIGITS as usize {
        return Some(value);
    }
    let source = crate::source_number::Decimal::parse(text)?;
    let serialized = serde_json::Number::from_f64(value)?.to_string();
    (source == crate::source_number::Decimal::parse(&serialized)?).then_some(value)
}

/// Parsed exact decimal ordering key. Reuse it when the same source cell participates
/// in many comparisons; this does not change coordinate range validation or formatting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecimalKey(crate::source_number::Decimal);

impl DecimalKey {
    pub fn parse(text: &str) -> Option<Self> {
        crate::source_number::Decimal::parse(text).map(Self)
    }
}

impl Ord for DecimalKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.compare(&other.0)
    }
}

impl PartialOrd for DecimalKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Exact ordering of decimal text, including values that share a rounded binary64 value.
/// Invalid syntax or an unsupported exponent returns `None`; coordinate range validation is unchanged.
pub fn compare_decimal_text(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    Some(DecimalKey::parse(left)?.cmp(&DecimalKey::parse(right)?))
}
