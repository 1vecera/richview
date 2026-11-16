use richview_chart::data::compare_decimal_text;
use std::cmp::Ordering::{Equal, Greater, Less};

#[test]
fn decimal_order_retains_distinct_source_values_and_normalizes_equivalent_notation() {
    for (left, right, expected) in [
        ("1.00e+3", "+1000", Equal),
        ("-0e999999999999999999999999", "0.0000", Equal),
        (" 1e0 ", "1.000", Equal),
        ("0", "1e-999", Less),
        ("-1e-999", "0", Less),
        ("9.999e100", "1e101", Less),
        ("-1.000000000000000000001", "-1.0", Less),
        ("9007199254740993", "9007199254740992", Greater),
        ("0.100000000000000005", "0.100000000000000004", Greater),
        ("1.00000000000000000001e-308", "1e-308", Greater),
    ] {
        assert_eq!(compare_decimal_text(left, right), Some(expected), "{left} / {right}");
        assert_eq!(compare_decimal_text(right, left), Some(expected.reverse()));
    }
}

#[test]
fn invalid_decimal_text_does_not_acquire_a_numeric_order() {
    for value in ["", "--1", "1e", "1e--2", "1.2.3", "inf", "NaN", "1_000"] {
        assert_eq!(compare_decimal_text(value, "1"), None, "{value}");
        assert_eq!(compare_decimal_text("1", value), None, "{value}");
    }
}
