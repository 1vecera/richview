//! The two precisions of FEA-CH-37: a value printed on its mark keeps the label's digits
//! (`digits`, three by default), and only its readout carries the author's `readout_digits`.
//! A chart with one precision prints exactly what its readout says.

use super::common;

use common::*;
use richview_chart::geometry::{Face, Geometry, Item, Role};
use serde_json::{Value, json};

fn fixture(name: &str) -> (Value, Vec<Value>) {
    let path = format!("{}/tests/fixtures/types/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
    (v["spec"].clone(), v["rows"].as_array().expect("rows").clone())
}

/// Every number in the rows times a factor with many digits, so six digits and three differ.
fn fine(rows: &[Value]) -> Vec<Value> {
    fn scale(v: &Value) -> Value {
        match v {
            Value::Number(n) => json!(n.as_f64().expect("finite") * 1.012_345_7),
            Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), scale(v))).collect()),
            Value::Array(a) => Value::Array(a.iter().map(scale).collect()),
            other => other.clone(),
        }
    }
    rows.iter().map(scale).collect()
}

fn with(spec: &Value, extra: Value) -> Value {
    let mut s = spec.clone();
    for (k, v) in extra.as_object().expect("object") {
        s[k] = v.clone();
    }
    s
}

/// The automatic values printed on the plot.
fn printed(g: &Geometry) -> Vec<String> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label && t.face == Face::Figures => Some(t.text.clone()),
            _ => None,
        })
        .collect()
}

fn readouts(g: &Geometry) -> Vec<String> {
    observations(g)
        .iter()
        .flat_map(|o| &o.readout.lines)
        .map(|l| l.value.clone())
        .collect()
}

/// The significant digits of the number in `text`, trailing zeros of an integer part aside.
fn significant(text: &str) -> usize {
    let digits: String = text
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '\u{202f}' | '\u{a0}' | ' '))
        .filter(char::is_ascii_digit)
        .collect();
    let digits = digits.trim_start_matches('0');
    let fraction = text.contains('.');
    if fraction {
        digits.len()
    } else {
        digits.trim_end_matches('0').len()
    }
}

fn families() -> Vec<(&'static str, Value, Vec<Value>)> {
    let sales = |n: usize| -> Vec<Value> {
        (0..n)
            .map(|i| json!({"m": format!("M{i}"), "v": 1234.5 + 811.3 * i as f64}))
            .collect()
    };
    let mut out = vec![
        (
            "bar",
            json!({"type":"bar","title":"T","encoding":{"x":"m","y":"v"}}),
            sales(4),
        ),
        (
            "line",
            json!({"type":"line","title":"T","encoding":{"x":"m","y":"v"}}),
            sales(4),
        ),
        (
            "area",
            json!({"type":"area","title":"T","encoding":{"x":"m","y":"v"}}),
            sales(4),
        ),
        (
            "boxplot",
            json!({"type":"boxplot","title":"T","encoding":{"x":"g","y":"v"}}),
            (0..30)
                .map(|i| json!({"g": if i % 2 == 0 { "A" } else { "B" }, "v": 1000.0 + 37.31 * i as f64}))
                .collect(),
        ),
    ];
    for name in [
        "combo",
        "treemap",
        "bullet",
        "dumbbell",
        "candlestick",
        "density",
        "gauge",
    ] {
        let (spec, rows) = fixture(name);
        out.push((name, spec, fine(&rows)));
    }
    out
}

#[test]
fn printed_values_keep_three_digits_while_readouts_carry_six() {
    for (name, spec, rows) in families() {
        let two = draw_at(&with(&spec, json!({"digits":3,"readout_digits":6})), &rows, 1440.0);
        let six = draw_at(&with(&spec, json!({"digits":6})), &rows, 1440.0);
        let three = draw_at(&with(&spec, json!({"digits":3})), &rows, 1440.0);
        let shown = printed(&two);
        assert!(!shown.is_empty(), "{name} prints values");
        for t in &shown {
            // Three digits, or four for a whole number below 10,000 (FEA-CH-22).
            assert!(significant(t) <= 4, "{name} prints {t} with more than three digits");
        }
        assert_eq!(shown, printed(&three), "{name} prints as with three digits alone");
        assert_ne!(
            shown,
            printed(&six),
            "{name}'s six-digit labels differ, so the test bites"
        );
        // Each printed value is the printed form of a readout line, which a dropped value's
        // record and the runtime's restore read back.
        let forms: Vec<&str> = observations(&two)
            .iter()
            .flat_map(|o| &o.readout.lines)
            .map(|l| l.printed())
            .collect();
        for t in &shown {
            assert!(
                forms.iter().any(|f| f == t || f.ends_with(&format!(" {t}"))),
                "{name} prints {t}, which no readout line prints: {forms:?}"
            );
        }
        assert_eq!(readouts(&two), readouts(&six), "{name} reads out at six digits");
        assert_ne!(readouts(&two), readouts(&three), "{name}'s readouts gain digits");
    }
}

#[test]
fn one_precision_prints_what_the_readout_says() {
    for (name, spec, rows) in families() {
        for digits in [2, 3, 5] {
            let g = draw_at(&with(&spec, json!({ "digits": digits })), &rows, 1440.0);
            let lines = readouts(&g);
            for t in printed(&g) {
                assert!(
                    lines.iter().any(|l| l == &t || l.ends_with(&format!(" {t}"))),
                    "{name} at {digits} digits prints {t}, which no readout line says: {lines:?}"
                );
            }
        }
    }
}
