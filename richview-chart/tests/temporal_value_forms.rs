//! The date and time forms a calendar axis accepts, and the precision each carries (TYP-90).

use richview_chart::time::{Precision, ms_from_civil, parse_iso};

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;

fn at(text: &str) -> Option<(i64, Precision)> {
    parse_iso(text)
}

#[test]
fn each_form_has_its_precision() {
    let day = ms_from_civil(2020, 3, 31);
    assert_eq!(at("2020"), Some((ms_from_civil(2020, 1, 1), Precision::Year)));
    assert_eq!(at("2020-03"), Some((ms_from_civil(2020, 3, 1), Precision::Month)));
    assert_eq!(at("2020-03-31"), Some((day, Precision::Day)));
    for separator in ["T", "t", " "] {
        assert_eq!(
            at(&format!("2020-03-31{separator}14:05")),
            Some((day + 14 * HOUR + 5 * MINUTE, Precision::Minute)),
            "separator {separator:?}"
        );
    }
    assert_eq!(
        at("2020-03-31 14:05:09"),
        Some((day + 14 * HOUR + 5 * MINUTE + 9_000, Precision::Second))
    );
    for fraction in [".5", ",5", ".500", ".5009999", ".123456789"] {
        let ms = at(&format!("2020-03-31T14:05:09{fraction}")).expect(fraction);
        assert_eq!(ms.1, Precision::Millisecond, "{fraction}");
    }
    assert_eq!(at("2020-03-31T14:05:09.5").unwrap().0 % 1000, 500);
    assert_eq!(
        at("2020-03-31T14:05:09.123456789").unwrap().0 % 1000,
        123,
        "digits beyond three are dropped"
    );
}

#[test]
fn offsets_shift_to_utc_and_no_offset_means_utc() {
    let utc = at("2020-03-31T14:05Z").unwrap().0;
    assert_eq!(at("2020-03-31T14:05z").unwrap().0, utc);
    assert_eq!(at("2020-03-31T14:05").unwrap().0, utc);
    assert_eq!(at("2020-03-31T16:05+02").unwrap().0, utc);
    assert_eq!(at("2020-03-31T16:05+0200").unwrap().0, utc);
    assert_eq!(at("2020-03-31T16:05+02:00").unwrap().0, utc);
    assert_eq!(at("2020-03-31T09:35-04:30").unwrap().0, utc);
    assert_eq!(at("2020-03-31 14:05:09,25Z").unwrap().1, Precision::Millisecond);
    assert_eq!(at("  2020-03-31  "), at("2020-03-31"), "surrounding spaces are ignored");
}

#[test]
fn out_of_range_and_other_forms_are_not_dates() {
    for text in [
        "2020-13-01",
        "2020-00-10",
        "2020-02-30",
        "2019-02-29",
        "2020-03-31T25:00",
        "2020-03-31T24:00",
        "2020-03-31T14:60",
        "2020-03-31T14:05:60",
        "2020-03-31T14:05+24:00",
        "2020-03-31T14:05+02:60",
        "2020-03-31T14:05:09.",
        "2020-03-31T14:05:09.1234567890",
        "2020-03-31T14",
        "2020-03-31T",
        "2020-3-31",
        "20-03-31",
        "2020/03/31",
        "March 2020",
        "2020-03-31Z",
        "",
        "n/a",
    ] {
        assert_eq!(at(text), None, "{text:?}");
    }
}
