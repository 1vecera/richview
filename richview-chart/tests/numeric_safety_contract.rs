//! Numbers from source text: a nonzero value too small to represent is an error, never a zero
//! observation, and a real zero, a missing marker and a representable subnormal stay what they
//! are (FEA-CH-70, TYP-91).

use richview_chart::rows::{Num, num};
use serde_json::json;

fn read(text: &str) -> Num {
    num(&json!({"v": text}), "v")
}

#[test]
fn nonzero_text_must_not_become_a_zero_observation() {
    for text in ["1e-400", "-1e-400", "2e-324", "1E-9999", "+7.2e-999"] {
        assert!(
            matches!(read(text), Num::Underflow(ref t) if t == text),
            "{text}: underflow needs an explicit diagnostic"
        );
    }
}

#[test]
fn real_zero_missing_markers_and_representable_subnormals_stay_valid() {
    for text in ["0", "0e-9999", "-0e-9999", "0.000", "+0e400"] {
        assert!(matches!(read(text), Num::Value(v) if v == 0.0), "{text}");
    }
    for text in ["", "na", ".", "NaN"] {
        assert!(matches!(read(text), Num::Missing), "{text}");
    }
    assert!(matches!(read("5e-324"), Num::Value(v) if v == f64::from_bits(1)));
    assert!(matches!(read("-5e-324"), Num::Value(v) if v == -f64::from_bits(1)));
}

#[test]
fn source_chart_numeric_text_uses_the_same_representable_range() {
    for text in [r#""1e-400""#, r#""-1e-400""#, r#""2e-324""#] {
        assert_eq!(
            richview_chart::rmd_render::number(&text.parse().unwrap()),
            None,
            "{text}"
        );
    }
    for (text, expected) in [
        (r#""0e-9999""#, 0.0),
        (r#""5e-324""#, f64::from_bits(1)),
        ("12.5", 12.5),
    ] {
        assert_eq!(
            richview_chart::rmd_render::number(&text.parse().unwrap()),
            Some(expected)
        );
    }
    assert_eq!(
        richview_chart::rmd_render::number(&r#"" 1 ""#.parse().unwrap()),
        None,
        "Keep the existing untrimmed source-chart numeric contract"
    );
}
