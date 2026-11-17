//! Dates and times for stored records, files, headers and pages, in exactly the text chrono 0.4
//! wrote and accepted before RichView moved to jiff (S27-986, RUSTSEC-2026-0543: chrono is
//! unmaintained). jiff does the calendar and the clock; this module keeps the old shapes:
//! RFC 3339 with `Z` at millisecond or second precision, chrono's strict RFC 3339 grammar, its
//! automatic fraction (0, 3, 6 or 9 digits), its signed years and its leap-second order. The test
//! below replays chrono's own output for every shape (`tests/fixtures/chrono-golden.tsv`).
//!
//! jiff's calendar ends at the years -9999 and 9999. chrono's went on to -262143 and 262142, and a
//! DuckDB date or timestamp from an author's data can lie out there, so between the two ends a small
//! calendar of its own (`beyond_jiff`) gives chrono's text, `+10000-01-01`. Past chrono's ends a value
//! has no text, as before, and DuckDB reports it as an unsupported value. The clock, invitation
//! expiries and the stamps read back from records never leave jiff's calendar.

use jiff::{Span, Timestamp, civil::Date};
use std::fmt::Write as _;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// An instant as whole seconds since 1970 and nanoseconds, ordered as chrono ordered it: a leap
/// second (`:60`) keeps 1,000,000,000 or more nanoseconds on the second before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
    seconds: i64,
    nanos: u32,
}

/// An RFC 3339 text read the way chrono's `parse_from_rfc3339` read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rfc3339 {
    /// The instant it names.
    pub instant: Instant,
    /// The calendar date as written, in the text's own offset.
    pub date: Date,
}

impl Instant {
    /// Now, from the system clock.
    pub fn now() -> Self {
        Self::from_timestamp(Timestamp::now())
    }

    /// `seconds` since 1970 and `nanos` within it, or none past chrono's calendar.
    pub fn from_unix(seconds: i64, nanos: u32) -> Option<Self> {
        (nanos < 1_000_000_000 && utc(seconds).is_some()).then_some(Self { seconds, nanos })
    }

    fn from_timestamp(at: Timestamp) -> Self {
        let total = at.as_nanosecond();
        Self {
            seconds: total.div_euclid(1_000_000_000) as i64,
            nanos: total.rem_euclid(1_000_000_000) as u32,
        }
    }

    /// Whole seconds since 1970.
    pub fn unix_seconds(self) -> i64 {
        self.seconds
    }

    /// Whole seconds from `earlier` to this instant, truncated toward zero like chrono's
    /// `num_seconds`.
    pub fn seconds_since(self, earlier: Self) -> i64 {
        let gap = (i128::from(self.seconds) - i128::from(earlier.seconds)) * 1_000_000_000
            + i128::from(self.nanos)
            - i128::from(earlier.nanos);
        (gap / 1_000_000_000) as i64
    }

    /// `2026-11-17T10:46:21.123Z`: the stamp on stored records (millisecond precision, truncated).
    pub fn millis(self) -> String {
        let mut text = self.utc_seconds_text();
        let _ = write!(text, ".{:03}Z", self.nanos.min(999_999_999) / 1_000_000);
        text
    }

    /// `2026-11-17T10:46:21Z`: a data snapshot's `fetched_at`.
    pub fn seconds(self) -> String {
        self.utc_seconds_text() + "Z"
    }

    /// `2024-01-02T03:04:05.123456Z`, with as many fraction digits as the value needs (0, 3, 6 or 9).
    pub fn rfc3339_auto(self) -> String {
        self.naive_auto() + "Z"
    }

    /// `2024-01-02T03:04:05.123456`: the same without a zone, for a timestamp that has none.
    pub fn naive_auto(self) -> String {
        let mut text = self.utc_seconds_text();
        let nanos = self.nanos;
        if nanos == 0 {
        } else if nanos.is_multiple_of(1_000_000) {
            let _ = write!(text, ".{:03}", nanos / 1_000_000);
        } else if nanos.is_multiple_of(1_000) {
            let _ = write!(text, ".{:06}", nanos / 1_000);
        } else {
            let _ = write!(text, ".{nanos:09}");
        }
        text
    }

    /// `2 Jan 2024, 03:04 UTC`.
    pub fn utc_minute(self) -> Option<String> {
        let (date, second_of_day) = utc(self.seconds)?;
        Some(format!(
            "{}, {:02}:{:02} UTC",
            day_month_year(date),
            second_of_day / 3600,
            second_of_day / 60 % 60
        ))
    }

    /// `2 Jan 2024`, the date in UTC.
    pub fn utc_day(self) -> Option<String> {
        utc(self.seconds).map(|(date, _)| day_month_year(date))
    }

    fn utc_seconds_text(self) -> String {
        let (date, second) = utc(self.seconds)
            .expect("an instant is inside chrono's calendar: from_unix checks it, now and parsing stay in it");
        format!(
            "{}T{:02}:{:02}:{:02}",
            iso_date(date),
            second / 3600,
            second / 60 % 60,
            second % 60
        )
    }
}

/// A calendar day with chrono's range of years, -262143 to 262142.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Day {
    year: i32,
    month: u8,
    day: u8,
}

impl From<Date> for Day {
    fn from(date: Date) -> Self {
        Self {
            year: i32::from(date.year()),
            month: date.month() as u8,
            day: date.day() as u8,
        }
    }
}

/// The day `days` after 1970-01-01: from jiff inside its calendar, beyond it from `beyond_jiff`,
/// and none past chrono's.
fn day_after_epoch(days: i64) -> Option<Day> {
    Span::new()
        .try_days(days)
        .ok()
        .and_then(|span| Date::constant(1970, 1, 1).checked_add(span).ok())
        .map(Day::from)
        .or_else(|| beyond_jiff(days))
}

/// The proleptic Gregorian day `days` after 1970-01-01 (Howard Hinnant's `civil_from_days`), for
/// the years jiff does not hold, limited to chrono's.
fn beyond_jiff(days: i64) -> Option<Day> {
    let z = days.checked_add(719_468)?;
    let era = z.div_euclid(146_097);
    let of_era = z.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * of_year + 2) / 153;
    let day = of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (-262_143..=262_142).contains(&year).then_some(Day {
        year: year as i32,
        month: month as u8,
        day: day as u8,
    })
}

/// The date and the second of the day in UTC, or none past chrono's calendar.
fn utc(seconds: i64) -> Option<(Day, i64)> {
    Some((
        day_after_epoch(seconds.div_euclid(86_400))?,
        seconds.rem_euclid(86_400),
    ))
}

/// Days from 1970-01-01 to `date`.
fn epoch_days(date: Date) -> i64 {
    i64::from(
        Date::constant(1970, 1, 1)
            .until(date)
            .expect("two dates of jiff's calendar are always comparable")
            .get_days(),
    )
}

/// `days` after 1970-01-01 as `2024-01-02`, or none past chrono's calendar.
pub fn date_after_epoch(days: i64) -> Option<String> {
    day_after_epoch(days).map(iso_date)
}

/// `2024-01-02`, with chrono's year: four digits, a `-` before a year below zero and a `+` before
/// one above 9999.
pub fn iso_date(day: impl Into<Day>) -> String {
    let day = day.into();
    format!("{}-{:02}-{:02}", year(day), day.month, day.day)
}

/// `2 Jan 2024`.
pub fn day_month_year(day: impl Into<Day>) -> String {
    let day = day.into();
    format!(
        "{} {} {}",
        day.day,
        MONTHS[usize::from(day.month - 1)],
        year(day)
    )
}

fn year(day: Day) -> String {
    match day.year {
        year if year < 0 => format!("-{:04}", -year),
        year if year > 9999 => format!("+{year}"),
        year => format!("{year:04}"),
    }
}

/// The date in the first ten characters of `text`, read as chrono's `%Y-%m-%d` read them: each
/// number may follow white space, has at least one digit and at most two (four for a year without
/// a sign; a signed year takes every digit), and nothing may follow the day.
pub fn leading_date(text: &str) -> Option<Date> {
    fn number(s: &str, most: usize) -> Option<(i64, &str)> {
        let count = s.bytes().take(most).take_while(u8::is_ascii_digit).count();
        (count > 0)
            .then(|| (s[..count].parse().ok(), &s[count..]))
            .and_then(|(n, rest)| Some((n?, rest)))
    }
    let s = text.get(..10)?.trim_start();
    let (year, s) = if let Some(rest) = s.strip_prefix('-') {
        number(rest, usize::MAX).map(|(n, rest)| (-n, rest))?
    } else if let Some(rest) = s.strip_prefix('+') {
        number(rest, usize::MAX)?
    } else {
        number(s, 4)?
    };
    let (month, s) = number(s.strip_prefix('-')?.trim_start(), 2)?;
    let (day, s) = number(s.strip_prefix('-')?.trim_start(), 2)?;
    if !s.is_empty() {
        return None;
    }
    Date::new(
        i16::try_from(year).ok()?,
        i8::try_from(month).ok()?,
        i8::try_from(day).ok()?,
    )
    .ok()
}

/// RFC 3339 read with chrono's grammar: `YYYY-MM-DD`, then `T`, `t` or a space, `HH:MM:SS`, an
/// optional `.` with one or more digits (past nine ignored), then `Z`, `z` or `±HH:MM` under 24
/// hours (the sign may be U+2212). `:60` is a leap second.
pub fn parse_rfc3339(text: &str) -> Option<Rfc3339> {
    let bytes = text.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let digits = |at: usize| -> Option<u32> {
        let (a, b) = (bytes[at], bytes[at + 1]);
        (a.is_ascii_digit() && b.is_ascii_digit())
            .then(|| u32::from(a - b'0') * 10 + u32::from(b - b'0'))
    };
    let year = digits(0)? * 100 + digits(2)?;
    if bytes[4] != b'-' || bytes[7] != b'-' || !matches!(bytes[10], b'T' | b't' | b' ') {
        return None;
    }
    if bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    let date = Date::new(year as i16, digits(5)? as i8, digits(8)? as i8).ok()?;
    let (hour, minute, mut second) = (digits(11)?, digits(14)?, digits(17)?);
    let mut nanos = 0u32;
    if second == 60 {
        second = 59;
        nanos = 1_000_000_000;
    }
    let mut rest = &text[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let count = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if count == 0 {
            return None;
        }
        let kept = &fraction[..count.min(9)];
        nanos += kept.parse::<u32>().ok()? * 10u32.pow(9 - kept.len() as u32);
        rest = &fraction[count..];
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let offset = match rest.as_bytes().first()? {
        b'Z' | b'z' => {
            rest = &rest[1..];
            0
        }
        _ => {
            let (negative, signed) = if let Some(r) = rest.strip_prefix('+') {
                (false, r)
            } else if let Some(r) = rest.strip_prefix('-') {
                (true, r)
            } else {
                (true, rest.strip_prefix('\u{2212}')?)
            };
            let b = signed.as_bytes();
            if b.len() < 5 || b[2] != b':' {
                return None;
            }
            let pair = |i: usize| {
                (b[i].is_ascii_digit() && b[i + 1].is_ascii_digit())
                    .then(|| i32::from(b[i] - b'0') * 10 + i32::from(b[i + 1] - b'0'))
            };
            let (hours, minutes) = (pair(0)?, pair(3)?);
            if minutes > 59 {
                return None;
            }
            let seconds = hours * 3600 + minutes * 60;
            if seconds >= 86_400 {
                return None;
            }
            rest = &signed[5..];
            if negative { -seconds } else { seconds }
        }
    };
    if !rest.is_empty() {
        return None;
    }
    let seconds = epoch_days(date) * 86_400 + i64::from(hour * 3600 + minute * 60 + second)
        - i64::from(offset);
    Some(Rfc3339 {
        instant: Instant { seconds, nanos },
        date,
    })
}

/// An HTTP `Date` header (RFC 2822), or none when it does not read.
pub fn parse_http_date(text: &str) -> Option<Instant> {
    jiff::fmt::rfc2822::DateTimeParser::new()
        .parse_timestamp(text)
        .ok()
        .map(Instant::from_timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant(input: &str) -> Option<Instant> {
        let (seconds, nanos) = input.split_once(',')?;
        Instant::from_unix(seconds.parse().ok()?, nanos.parse().ok()?)
    }

    /// The text this module gives for one golden case; `<none>` where it has none.
    fn after(kind: &str, input: &str) -> String {
        const NONE: &str = "<none>";
        let text = |value: Option<String>| value.unwrap_or_else(|| NONE.into());
        match kind {
            "millis" => text(instant(input).map(Instant::millis)),
            "seconds" => text(instant(input).map(Instant::seconds)),
            "duck_date" => text(date_after_epoch(input.parse().unwrap())),
            "duck_timestamp" => {
                let parts: Vec<&str> = input.split(',').collect();
                let per_second: i64 = match parts[0] {
                    "s" => 1,
                    "ms" => 1_000,
                    "us" => 1_000_000,
                    _ => 1_000_000_000,
                };
                let value: i64 = parts[1].parse().unwrap();
                let at = Instant::from_unix(
                    value.div_euclid(per_second),
                    (value.rem_euclid(per_second) * (1_000_000_000 / per_second)) as u32,
                );
                text(at.map(|at| match parts[2] {
                    "zoned" => at.rfc3339_auto(),
                    _ => at.naive_auto(),
                }))
            }
            "rfc3339" => text(parse_rfc3339(input).map(|parsed| {
                format!(
                    "{}.{:09} {} {}",
                    parsed.instant.seconds,
                    parsed.instant.nanos,
                    iso_date(parsed.date),
                    if input.len() >= 20 {
                        "publishable"
                    } else {
                        "short"
                    }
                )
            })),
            "token_date" => parse_rfc3339(input)
                .and_then(|parsed| parsed.instant.utc_minute())
                .unwrap_or_else(|| input.to_owned()),
            "report_date" => leading_date(input).map_or_else(|| input.to_owned(), day_month_year),
            "human_date" => parse_rfc3339(input).map_or_else(
                || input.chars().take(10).collect(),
                |parsed| day_month_year(parsed.date),
            ),
            "expiry" => {
                let expires: u64 = input.parse().unwrap();
                Instant::from_unix(expires as i64, 0)
                    .and_then(Instant::utc_day)
                    .unwrap_or_default()
            }
            "skew" => {
                let (now, header) = input.split_once('|').unwrap();
                let now = instant(now).unwrap();
                parse_http_date(header)
                    .map(|date| now.seconds_since(date))
                    .map_or(String::new(), |s| format!(" by {s} s"))
            }
            _ => panic!("unknown kind {kind}"),
        }
    }

    #[test]
    fn every_shape_chrono_wrote_or_read_comes_out_the_same() {
        let golden = include_str!("../tests/fixtures/chrono-golden.tsv");
        for line in golden.lines() {
            let mut fields = line.split('\t');
            let (kind, input, before) = (
                fields.next().unwrap(),
                fields.next().unwrap(),
                fields.next().unwrap_or(""),
            );
            assert_eq!(after(kind, input), before, "{kind} {input}");
        }
        assert_eq!(golden.lines().count(), 350);
    }

    #[test]
    fn the_fallback_calendar_agrees_with_jiff_where_both_hold() {
        for days in (-4_371_587..=2_932_896)
            .step_by(9_973)
            .chain([-4_371_587, 0, 59, 60, 2_932_896])
        {
            let jiff = Date::constant(1970, 1, 1)
                .checked_add(Span::new().days(days))
                .unwrap();
            assert_eq!(beyond_jiff(days), Some(Day::from(jiff)), "{days}");
        }
    }

    #[test]
    fn an_authors_year_past_9999_keeps_chronos_text() {
        assert_eq!(date_after_epoch(2_932_897).unwrap(), "+10000-01-01");
        assert_eq!(date_after_epoch(-4_371_588).unwrap(), "-10000-12-31");
        let at = Instant::from_unix(253_402_300_800, 120_000_000).unwrap();
        assert_eq!(at.rfc3339_auto(), "+10000-01-01T00:00:00.120Z");
        assert_eq!(at.naive_auto(), "+10000-01-01T00:00:00.120");
        // Past chrono's calendar there is still no text; the DuckDB adapter reports the value.
        assert_eq!(date_after_epoch(95_026_237), None);
        assert_eq!(Instant::from_unix(8_210_266_876_800, 0), None);
    }

    #[test]
    fn stored_stamps_read_back_and_write_again_unchanged() {
        // Records in the hosted-v2 store and the account files carry `millis` stamps; data
        // snapshots carry `seconds`. These are the shapes in today's records and fixtures.
        for stored in [
            "2026-11-11T15:25:47.155Z",
            "2026-01-01T00:00:00.000Z",
            "2026-01-01T00:02:00.000Z",
            "1970-01-01T00:00:00.001Z",
            "2026-12-31T23:59:59.999Z",
        ] {
            let read = parse_rfc3339(stored).unwrap();
            assert_eq!(read.instant.millis(), stored);
            assert_eq!(iso_date(read.date), &stored[..10]);
        }
        for stored in [
            "2026-10-29T12:00:00Z",
            "2000-01-01T00:00:00Z",
            "2024-02-29T23:59:59Z",
        ] {
            assert_eq!(parse_rfc3339(stored).unwrap().instant.seconds(), stored);
        }
        // A publication date keeps the calendar day its author wrote, in its own offset.
        let published = parse_rfc3339("2026-10-08T23:30:00-02:00").unwrap();
        assert_eq!(day_month_year(published.date), "8 Oct 2026");
        assert_eq!(
            published.instant.utc_minute().unwrap(),
            "9 Oct 2026, 01:30 UTC"
        );
    }

    #[test]
    fn the_clock_writes_the_stored_shapes() {
        let now = Instant::now();
        let millis = now.millis();
        assert_eq!(millis.len(), 24, "{millis}");
        assert!(millis.ends_with('Z') && millis.as_bytes()[19] == b'.');
        assert_eq!(now.seconds().len(), 20);
        let read = parse_rfc3339(&millis).unwrap();
        assert_eq!(read.instant.seconds, now.seconds);
        assert_eq!(read.instant.nanos, now.nanos / 1_000_000 * 1_000_000);
    }
}
