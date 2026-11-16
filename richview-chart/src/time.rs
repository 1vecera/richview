//! Calendar arithmetic in UTC: ISO-8601 parsing and calendar-aligned tick intervals.
//!
//! Instants are milliseconds since the Unix epoch (UTC) as `i64`. Civil dates use Howard
//! Hinnant's days-from-civil algorithms, exact for the proleptic Gregorian calendar, so month
//! lengths, leap years and week starts are computed rather than approximated.

pub const MS_SECOND: i64 = 1_000;
pub const MS_MINUTE: i64 = 60 * MS_SECOND;
pub const MS_HOUR: i64 = 60 * MS_MINUTE;
pub const MS_DAY: i64 = 24 * MS_HOUR;

/// Days since 1970-01-01 for a proleptic Gregorian civil date.
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Civil date `(year, month, day)` for a count of days since 1970-01-01.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

pub fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Broken-down UTC fields of an instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millis: u32,
    /// ISO weekday index, 0 = Monday.
    pub weekday: u32,
}

pub fn civil(ms: i64) -> Civil {
    let days = ms.div_euclid(MS_DAY);
    let rem = ms.rem_euclid(MS_DAY);
    let (year, month, day) = civil_from_days(days);
    Civil {
        year,
        month,
        day,
        hour: (rem / MS_HOUR) as u32,
        minute: ((rem % MS_HOUR) / MS_MINUTE) as u32,
        second: ((rem % MS_MINUTE) / MS_SECOND) as u32,
        millis: (rem % MS_SECOND) as u32,
        weekday: (days + 3).rem_euclid(7) as u32,
    }
}

pub fn ms_from_civil(year: i64, month: u32, day: u32) -> i64 {
    days_from_civil(year, month, day) * MS_DAY
}

/// Add calendar months, clamping the day to the target month (31 Mar - 1 month = 28/29 Feb).
pub fn add_months(ms: i64, months: i64) -> i64 {
    let c = civil(ms);
    let index = c.year * 12 + i64::from(c.month) - 1 + months;
    let year = index.div_euclid(12);
    let month = (index.rem_euclid(12) + 1) as u32;
    let day = c.day.min(days_in_month(year, month));
    ms_from_civil(year, month, day) + ms.rem_euclid(MS_DAY)
}

/// How precisely a source timestamp was written; decides tooltip date formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Precision {
    Year,
    Month,
    Day,
    Minute,
    Second,
    Millisecond,
}

fn digits(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Parse an ISO-8601 date or date-time into UTC milliseconds.
///
/// Accepts `YYYY`, `YYYY-MM`, `YYYY-MM-DD` and `YYYY-MM-DD[T ]HH:MM[:SS[.fff]]` with an optional `Z`
/// or `+HH:MM`/`-HHMM` offset. Date-times without an offset are read as UTC. Out-of-range fields
/// (month 13, 30 February, 25:00) are rejected instead of normalised.
pub fn parse_iso(text: &str) -> Option<(i64, Precision)> {
    let s = text.trim();
    let (date, rest) = match s.find(['T', 't', ' ']) {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let mut parts = date.split('-');
    let year_text = parts.next()?;
    if year_text.len() != 4 {
        return None;
    }
    let year = digits(year_text)?;
    let month = match parts.next() {
        Some(p) if p.len() == 2 => digits(p)? as u32,
        Some(_) => return None,
        None => {
            return if rest.is_none() {
                Some((ms_from_civil(year, 1, 1), Precision::Year))
            } else {
                None
            };
        }
    };
    if !(1..=12).contains(&month) {
        return None;
    }
    let day = match parts.next() {
        Some(p) if p.len() == 2 => digits(p)? as u32,
        Some(_) => return None,
        None => {
            return if rest.is_none() {
                Some((ms_from_civil(year, month, 1), Precision::Month))
            } else {
                None
            };
        }
    };
    if parts.next().is_some() || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let midnight = ms_from_civil(year, month, day);
    let Some(rest) = rest else {
        return Some((midnight, Precision::Day));
    };

    let (clock, offset_ms) = split_offset(rest)?;
    let mut fields = clock.split(':');
    let hour = digits_exact(fields.next()?, 2)?;
    let minute = digits_exact(fields.next()?, 2)?;
    let (second, millis, precision) = match fields.next() {
        None => (0, 0, Precision::Minute),
        Some(sec) => {
            let (whole, frac) = match sec.split_once(['.', ',']) {
                Some((w, f)) => (w, Some(f)),
                None => (sec, None),
            };
            let second = digits_exact(whole, 2)?;
            match frac {
                None => (second, 0, Precision::Second),
                Some(f) if !f.is_empty() && f.len() <= 9 && f.bytes().all(|b| b.is_ascii_digit()) => {
                    let padded = format!("{f:0<3}");
                    (second, padded[..3].parse::<i64>().ok()?, Precision::Millisecond)
                }
                Some(_) => return None,
            }
        }
    };
    if fields.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let ms = midnight + hour * MS_HOUR + minute * MS_MINUTE + second * MS_SECOND + millis - offset_ms;
    Some((ms, precision))
}

fn digits_exact(s: &str, len: usize) -> Option<i64> {
    if s.len() == len { digits(s) } else { None }
}

/// Split a trailing `Z`, `+HH:MM`, `+HHMM` or `+HH` offset from a clock string.
fn split_offset(clock: &str) -> Option<(&str, i64)> {
    if let Some(stripped) = clock.strip_suffix(['Z', 'z']) {
        return Some((stripped, 0));
    }
    let Some(i) = clock.rfind(['+', '-']) else {
        return Some((clock, 0));
    };
    let (body, offset) = (&clock[..i], &clock[i..]);
    let sign = if offset.starts_with('-') { -1 } else { 1 };
    let offset = &offset[1..];
    let (h, m) = match offset.len() {
        2 => (digits(offset)?, 0),
        4 => (digits(offset.get(..2)?)?, digits(offset.get(2..)?)?),
        5 if offset.as_bytes()[2] == b':' => (digits(offset.get(..2)?)?, digits(offset.get(3..)?)?),
        _ => return None,
    };
    if h > 23 || m > 59 {
        return None;
    }
    Some((body, sign * (h * MS_HOUR + m * MS_MINUTE)))
}

/// A calendar tick interval such as "every 3 months" or "every 15 minutes".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interval {
    pub unit: Unit,
    pub step: i64,
}

const fn iv(unit: Unit, step: i64) -> Interval {
    Interval { unit, step }
}

/// Candidate tick intervals, finest first. Every step divides its parent unit evenly so ticks stay
/// on round clock and calendar positions (quarters start in Jan/Apr/Jul/Oct, decades on years
/// divisible by ten, weeks on Mondays).
pub const INTERVALS: [Interval; 24] = [
    iv(Unit::Second, 1),
    iv(Unit::Second, 5),
    iv(Unit::Second, 15),
    iv(Unit::Second, 30),
    iv(Unit::Minute, 1),
    iv(Unit::Minute, 5),
    iv(Unit::Minute, 15),
    iv(Unit::Minute, 30),
    iv(Unit::Hour, 1),
    iv(Unit::Hour, 3),
    iv(Unit::Hour, 6),
    iv(Unit::Hour, 12),
    iv(Unit::Day, 1),
    iv(Unit::Week, 1),
    iv(Unit::Month, 1),
    iv(Unit::Month, 3),
    iv(Unit::Month, 6),
    iv(Unit::Year, 1),
    iv(Unit::Year, 2),
    iv(Unit::Year, 5),
    iv(Unit::Year, 10),
    iv(Unit::Year, 20),
    iv(Unit::Year, 50),
    iv(Unit::Year, 100),
];

impl Interval {
    /// The shortest real duration between two consecutive ticks, used for spacing checks.
    pub fn min_duration_ms(self) -> i64 {
        let s = self.step;
        match self.unit {
            Unit::Second => s * MS_SECOND,
            Unit::Minute => s * MS_MINUTE,
            Unit::Hour => s * MS_HOUR,
            Unit::Day => s * MS_DAY,
            Unit::Week => s * 7 * MS_DAY,
            Unit::Month => match s {
                1 => 28 * MS_DAY,
                3 => 90 * MS_DAY,
                _ => 181 * MS_DAY,
            },
            Unit::Year => s * 365 * MS_DAY,
        }
    }

    /// Largest tick at or before `ms`.
    pub fn floor(self, ms: i64) -> i64 {
        let fixed = |len: i64| ms.div_euclid(len) * len;
        match self.unit {
            Unit::Second => fixed(self.step * MS_SECOND),
            Unit::Minute => fixed(self.step * MS_MINUTE),
            Unit::Hour => fixed(self.step * MS_HOUR),
            Unit::Day => fixed(self.step * MS_DAY),
            Unit::Week => {
                let days = ms.div_euclid(MS_DAY);
                (days - (days + 3).rem_euclid(7)) * MS_DAY
            }
            Unit::Month => {
                let c = civil(ms);
                let index = c.year * 12 + i64::from(c.month) - 1;
                let aligned = index - index.rem_euclid(self.step);
                ms_from_civil(aligned.div_euclid(12), (aligned.rem_euclid(12) + 1) as u32, 1)
            }
            Unit::Year => {
                let year = civil(ms).year;
                ms_from_civil(year - year.rem_euclid(self.step), 1, 1)
            }
        }
    }

    /// The tick after `tick`, which must itself lie on this interval.
    pub fn next(self, tick: i64) -> i64 {
        match self.unit {
            Unit::Second => tick + self.step * MS_SECOND,
            Unit::Minute => tick + self.step * MS_MINUTE,
            Unit::Hour => tick + self.step * MS_HOUR,
            Unit::Day => tick + self.step * MS_DAY,
            Unit::Week => tick + self.step * 7 * MS_DAY,
            Unit::Month => add_months(tick, self.step),
            Unit::Year => add_months(tick, 12 * self.step),
        }
    }

    /// All ticks in `[from, to]`.
    pub fn ticks(self, from: i64, to: i64) -> Vec<i64> {
        let mut out = Vec::new();
        let mut t = self.floor(from);
        if t < from {
            t = self.next(t);
        }
        while t <= to && out.len() < 2_000 {
            out.push(t);
            t = self.next(t);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip_and_known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(civil_from_days(-719_468), (0, 3, 1));
        let mut z = -1_000_000;
        while z < 1_000_000 {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z, "round trip for day {z}");
            z += 997;
        }
    }

    #[test]
    fn weekdays_and_month_lengths() {
        assert_eq!(
            civil(ms_from_civil(2025, 6, 21)).weekday,
            5,
            "21 Jun 2025 is a Saturday"
        );
        assert_eq!(civil(ms_from_civil(2024, 1, 1)).weekday, 0, "1 Jan 2024 is a Monday");
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(add_months(ms_from_civil(2024, 3, 31), -1), ms_from_civil(2024, 2, 29));
        assert_eq!(add_months(ms_from_civil(2023, 1, 31), 1), ms_from_civil(2023, 2, 28));
    }

    #[test]
    fn parses_iso_dates_and_offsets() {
        assert_eq!(
            parse_iso("2026-08"),
            Some((ms_from_civil(2026, 8, 1), Precision::Month))
        );
        assert_eq!(parse_iso("1958"), Some((ms_from_civil(1958, 1, 1), Precision::Year)));
        assert_eq!(
            parse_iso("2024-02-29"),
            Some((ms_from_civil(2024, 2, 29), Precision::Day))
        );
        assert_eq!(parse_iso("2023-02-29"), None);
        assert_eq!(parse_iso("2025-13-01"), None);
        assert_eq!(parse_iso("2025-1-01"), None);
        let noon = ms_from_civil(2025, 6, 21) + 12 * MS_HOUR;
        assert_eq!(parse_iso("2025-06-21T12:00Z"), Some((noon, Precision::Minute)));
        assert_eq!(parse_iso("2025-06-21 14:00+02:00"), Some((noon, Precision::Minute)));
        assert_eq!(parse_iso("2025-06-21T07:30-0430"), Some((noon, Precision::Minute)));
        assert_eq!(
            parse_iso("2025-06-21T12:00:00.250Z"),
            Some((noon + 250, Precision::Millisecond))
        );
        assert_eq!(parse_iso("2025-06-21T24:00Z"), None);
        assert_eq!(parse_iso("June 2025"), None);
    }

    #[test]
    fn ticks_land_on_calendar_boundaries() {
        let from = ms_from_civil(2024, 11, 15);
        let to = ms_from_civil(2025, 3, 10);
        let months = iv(Unit::Month, 1).ticks(from, to);
        let expected: Vec<i64> = [(2024, 12), (2025, 1), (2025, 2), (2025, 3)]
            .iter()
            .map(|&(y, m)| ms_from_civil(y, m, 1))
            .collect();
        assert_eq!(months, expected);

        let quarters = iv(Unit::Month, 3).ticks(ms_from_civil(2023, 2, 1), ms_from_civil(2024, 1, 1));
        let qm: Vec<u32> = quarters.iter().map(|t| civil(*t).month).collect();
        assert_eq!(qm, vec![4, 7, 10, 1]);

        for t in iv(Unit::Week, 1).ticks(ms_from_civil(2025, 6, 1), ms_from_civil(2025, 8, 1)) {
            assert_eq!(civil(t).weekday, 0, "weekly ticks fall on Mondays");
        }
        let decades = iv(Unit::Year, 10).ticks(ms_from_civil(1958, 3, 1), ms_from_civil(2026, 8, 1));
        let years: Vec<i64> = decades.iter().map(|t| civil(*t).year).collect();
        assert_eq!(years, vec![1960, 1970, 1980, 1990, 2000, 2010, 2020]);
    }
}
