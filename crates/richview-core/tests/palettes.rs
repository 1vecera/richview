//! Chart colour contracts of the builtin themes (director decisions D25 and D26).
//!
//! Every theme's categorical palette is authored in OKLCH, inside sRGB, with the theme accent as
//! series 1 (hand-drawn reserves its red accent for the stat underline). Its first four colours reach 3:1 against the theme's chart surface and page, and
//! every pair of neighbouring colours differs by an OKLab ΔE×100 of at least 15 for normal vision
//! and at least 8 under protan, deutan and tritan simulation. Each heatmap ramp is three OKLCH
//! stops whose lightness moves steadily away from the surface, so salience rises with value, and
//! whose high end stands out by at least 3:1. The checks themselves live in
//! `richview_chart::color`.

use richview_chart::color::{self, Oklch};
use richview_core::{builtin_style_names, resolve_styles};

use serde_json::Value;

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_builtin_theme_has_a_validated_palette_and_a_rising_ramp() {
    let mut failures = vec![];
    for name in builtin_style_names() {
        let mut diagnostics = vec![];
        let style = resolve_styles(&[name.to_string()], &mut diagnostics);
        let tokens = &style.document["tokens"];
        let surfaces: Vec<(&str, Oklch)> = ["bg-chart", "bg"]
            .into_iter()
            .filter_map(|key| {
                tokens[key]
                    .as_str()
                    .and_then(color::parse)
                    .map(|c| (key, c))
            })
            .collect();
        assert!(!surfaces.is_empty(), "{name} defines its surfaces");
        let palette: Vec<String> = match &style.chart["palette"] {
            Value::String(named) => richview_chart::rmd_render::palette(named)
                .iter()
                .map(|c| c.to_string())
                .collect(),
            other => strings(other),
        };
        assert!(palette.len() >= 4, "{name} has a categorical palette");
        let palette: Vec<&str> = palette.iter().map(String::as_str).collect();
        failures.extend(
            color::palette_issues(&palette, &surfaces)
                .into_iter()
                .map(|issue| format!("{name}: {issue}")),
        );
        let accent = tokens["accent"].as_str().and_then(color::parse).unwrap();
        // Hand-drawn reserves its red pencil for emphasis; data starts in graphite.
        if *name != "hand-drawn" && color::parse(palette[0]).map(Oklch::hex) != Some(accent.hex()) {
            failures.push(format!(
                "{name}: series 1 {} is not the accent {}",
                palette[0],
                accent.hex()
            ));
        }
        let ramp = strings(&style.chart["ramp"]);
        assert_eq!(ramp.len(), 3, "{name} defines a three-stop chart.ramp");
        let ramp: Vec<&str> = ramp.iter().map(String::as_str).collect();
        failures.extend(
            color::ramp_issues(&ramp, &surfaces)
                .into_iter()
                .map(|issue| format!("{name}: {issue}")),
        );
        for entry in palette.iter().chain(&ramp) {
            if !entry.starts_with("oklch(") || !color::parse(entry).is_some_and(Oklch::in_gamut) {
                failures.push(format!(
                    "{name}: {entry} must be an oklch() colour inside sRGB"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn product_colours_are_authored_in_oklch() {
    use std::path::Path;

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let legacy = regex::Regex::new(
        r"(?:^|[^&\w#])#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?)\(\s*[-0-9.]|color-mix\(in (?:srgb|oklab)[, ]",
    )
    .unwrap();
    fn check(path: &Path, legacy: &regex::Regex, failures: &mut Vec<String>) {
        if path.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                check(&entry.unwrap().path(), legacy, failures);
            }
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs" | "css" | "yaml" | "html" | "json")
        ) {
            let source = std::fs::read_to_string(path).unwrap();
            let product = if path.extension().is_some_and(|e| e == "rs") {
                source.split("#[cfg(test)]").next().unwrap()
            } else {
                &source
            };
            for (line, text) in product.lines().enumerate() {
                if legacy.is_match(text) {
                    failures.push(format!("{}:{}", path.display(), line + 1));
                }
            }
        }
    }
    let mut failures = vec![];
    // The independently developed landing/reel remain owned by their existing PR.
    for relative in [
        "crates/richview-core/assets",
        "crates/richview-core/src",
        "richview-chart/src",
        "crates/richview-site/src/browser.rs",
        "crates/richview-site/src/browser/showcase.rs",
        "crates/richview-site/src/browser/authoring.rs",
        "assets/styles",
        "assets/showcase",
        "assets/docs",
        "assets/playground.html",
    ] {
        check(&root.join(relative), &legacy, &mut failures);
    }
    assert!(
        failures.is_empty(),
        "Author product colours in OKLCH:\n{}",
        failures.join("\n")
    );
}

#[test]
fn standalone_charts_fall_back_to_a_validated_light_palette_and_ramp() {
    let white = [("white", color::parse("#ffffff").unwrap())];
    let fallback = richview_chart::rmd_render::palette("");
    assert!(color::palette_issues(fallback, &white).is_empty());
    // A chart without a style's theme draws with the engine's default, the light `air` values.
    let theme = richview_chart::layout::ChartTheme::default();
    let palette: Vec<&str> = theme.palette.iter().map(String::as_str).collect();
    assert!(color::palette_issues(&palette, &white).is_empty());
    let ramp: Vec<&str> = theme.ramp.iter().map(String::as_str).collect();
    assert!(color::ramp_issues(&ramp, &white).is_empty());
}

#[test]
fn the_check_rejects_single_hue_and_low_contrast_tables() {
    // The categorical tables these themes shipped before D25.
    let white = [("bg-chart", color::parse("#ffffff").unwrap())];
    let paper = [("bg-chart", color::parse("oklch(0.985 0.004 70)").unwrap())];
    let scientific = [
        "#1B6F9E", "#5B9BD5", "#8FAADC", "#B4C7E7", "#D9E2F3", "#2C3E50",
    ];
    let sleek = [
        "#195BFF", "#00AFA0", "#D533C5", "#FF6648", "#BA7A00", "#7552E8",
    ];
    let scientific = color::palette_issues(&scientific, &paper);
    assert!(
        scientific
            .iter()
            .any(|f| f.contains("series 2 #5B9BD5 reaches 2.8"))
    );
    assert!(scientific.iter().any(|f| f.contains("normal vision")));
    let light = color::palette_issues(&sleek, &white);
    assert!(light.iter().any(|f| f.contains("#00AFA0 reaches 2.75")));
    assert!(light.iter().any(|f| f.contains("#FF6648 reaches 2.90")));
}
