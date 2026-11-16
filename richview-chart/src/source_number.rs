//! Exact decimal values of source number text, compared without rounding; plotted coordinates
//! remain binary64.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Decimal {
    negative: bool,
    digits: Vec<u8>,
    exponent: i64,
}

impl Decimal {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (negative, body) = if let Some(body) = text.strip_prefix('-') {
            (true, body)
        } else {
            (false, text.strip_prefix('+').unwrap_or(text))
        };
        let (mantissa, exponent) = body.split_once(['e', 'E']).unwrap_or((body, "0"));
        let (integer, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        if integer.is_empty() && fraction.is_empty()
            || !integer
                .bytes()
                .chain(fraction.bytes())
                .all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let exponent_digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent_digits.is_empty() || !exponent_digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let mut digits: Vec<_> = integer
            .bytes()
            .chain(fraction.bytes())
            .map(|byte| byte - b'0')
            .collect();
        let Some(first) = digits.iter().position(|&digit| digit != 0) else {
            return Some(Self {
                negative: false,
                digits: vec![0],
                exponent: 0,
            });
        };
        let last = digits.iter().rposition(|&digit| digit != 0)?;
        let trailing = digits.len() - last - 1;
        let exponent = exponent
            .parse::<i64>()
            .ok()?
            .checked_sub(fraction.len().try_into().ok()?)?
            .checked_add(trailing.try_into().ok()?)?;
        digits.truncate(last + 1);
        digits.drain(..first);
        Some(Self {
            negative,
            digits,
            exponent,
        })
    }

    pub(crate) fn compare(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        if self.negative != other.negative {
            return self.negative.cmp(&other.negative).reverse();
        }
        let order = match (self.digits == [0], other.digits == [0]) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => {
                let magnitude = self.digits.len() as i128 + i128::from(self.exponent);
                let other_magnitude = other.digits.len() as i128 + i128::from(other.exponent);
                magnitude.cmp(&other_magnitude).then_with(|| {
                    let length = self.digits.len().max(other.digits.len());
                    self.digits
                        .iter()
                        .copied()
                        .chain(std::iter::repeat(0))
                        .take(length)
                        .cmp(other.digits.iter().copied().chain(std::iter::repeat(0)).take(length))
                })
            }
        };
        if self.negative { order.reverse() } else { order }
    }
}
