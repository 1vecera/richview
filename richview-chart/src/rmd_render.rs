//! Chart-type options and named palettes that the block contract and the layout read: the
//! boxplot, histogram and interval option parsers and the categorical palettes. The source-format
//! painter that once lived here is gone: every chart draws from one geometry model (ARC-18,
//! S27-703).

use serde_json::Value;

mod boxplot;
pub use boxplot::options as boxplot_options;
mod histogram;
pub use histogram::options as histogram_options;
mod interval;
pub use interval::options as interval_options;

/// The finite numeric values accepted by source-chart geometry, without rewriting source text.
pub fn number(v: &Value) -> Option<f64> {
    let n = v
        .as_f64()
        .or_else(|| v.as_str().and_then(|s| crate::data::parse_finite_number(s).ok()))?;
    n.is_finite().then_some(n)
}

fn get<'a>(v: &'a Value, p: &str, default: &'a str) -> &'a str {
    v.pointer(p).and_then(Value::as_str).unwrap_or(default)
}

/// The categorical palette of a theme: eight colours in a fixed order, the theme accent first,
/// authored in OKLCH. Each is validated on its own chart surface by `color::palette_issues` (D25,
/// enforced for every builtin theme by `crates/richview-core/tests/palettes.rs`): the first four
/// colours reach 3:1 against the surface, every pair of them and each neighbouring pair after
/// them differ by an OKLab ΔE×100 of at least 15 for normal vision and 8 under protan, deutan and tritan simulation. `sleek` is stepped for its
/// dark surface and `air` for white. Any other name gives the light default that
/// standalone charts fall back to. Sequential encodings use `chart.ramp` instead.
pub fn palette(name: &str) -> &'static [&'static str] {
    match name {
        "hand-drawn" => &[
            "oklch(0.31 0.005 85)",
            "oklch(0.64 0.005 85)",
            "oklch(0.15 0.005 85)",
            "oklch(0.47 0.005 85)",
            "oklch(0.79 0.005 85)",
            "oklch(0.38 0.005 85)",
        ],
        "sleek" => &[
            "oklch(0.546 0.251 263.5)",
            "oklch(0.77 0.16 335)",
            "oklch(0.69 0.12 185)",
            "oklch(0.69 0.18 33)",
            "oklch(0.77 0.16 78)",
            "oklch(0.69 0.16 287)",
            "oklch(0.79 0.18 145)",
            "oklch(0.57 0.14 0)",
        ],
        "scientific" => &[
            "oklch(0.5196 0.1248 247.06)",
            "oklch(0.52 0.12 55)",
            "oklch(0.6 0.12 166)",
            "oklch(0.38 0.12 20)",
            "oklch(0.52 0.12 320)",
            "oklch(0.62 0.1 95)",
            "oklch(0.62 0.1 280)",
            "oklch(0.42 0.1 140)",
        ],
        "brutalist" => &[
            "oklch(0.413 0.123 255.6)",
            "oklch(0.52 0.13 50)",
            "oklch(0.5 0.1 164)",
            "oklch(0.54 0.13 310)",
            "oklch(0.4 0.13 25)",
            "oklch(0.62 0.11 85)",
            "oklch(0.61 0.09 200)",
            "oklch(0.52 0.17 357)",
        ],
        // Air: the accent blue, then calmer earth and ink hues, so its charts read differently
        // from Sleek's vivid set (STY-01, STY-23).
        "air" => &[
            "oklch(0.48 0.22 264)",
            "oklch(0.58 0.09 175)",
            "oklch(0.62 0.15 55)",
            "oklch(0.5 0.15 12)",
            "oklch(0.64 0.12 100)",
            "oklch(0.55 0.1 305)",
            "oklch(0.38 0.06 250)",
            "oklch(0.63 0.05 60)",
        ],
        _ => &[
            "oklch(0.48 0.22 264)",
            "oklch(0.64 0.1 188)",
            "oklch(0.56 0.2 335)",
            "oklch(0.56 0.2 33)",
            "oklch(0.66 0.14 72)",
            "oklch(0.58 0.12 287)",
            "oklch(0.56 0.18 142)",
            "oklch(0.66 0.24 3)",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn source_numbers_keep_the_geometry_parser_contract() {
        for (value, expected) in [
            (json!(4), 4.0),
            (json!("4.000"), 4.0),
            (json!("-2.5e1"), -25.0),
            (json!("+0.000"), 0.0),
            (json!("1e-310"), 1e-310),
        ] {
            assert_eq!(number(&value), Some(expected));
        }
        for value in [
            json!(null),
            json!(true),
            json!(""),
            json!(" 4.000 "),
            json!("phase"),
            json!("0x10"),
            json!("NaN"),
            json!("Infinity"),
            json!("1e999"),
        ] {
            assert_eq!(number(&value), None, "{value}");
        }
    }
}
