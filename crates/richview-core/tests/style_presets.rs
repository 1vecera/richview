//! Presets, the author's colours and the five-step settings (STY-73): every combination holds
//! the contrast floors (STY-25), the palette rule (STY-22) and the type scale (STY-26), old
//! vocabulary is ignored with a retirement warning, and `medium` changes nothing.

use richview_core::{
    EXTENDED_PRESETS, ResolvedStyle, STEP_SETTINGS, STEPS, builtin_style_names, preset_names,
    resolve_styles, style_checks,
};

fn styled(refs: &[&str]) -> (ResolvedStyle, Vec<richview_core::document::Diagnostic>) {
    let mut diagnostics = vec![];
    let refs: Vec<String> = refs.iter().map(|r| r.to_string()).collect();
    (resolve_styles(&refs, &mut diagnostics), diagnostics)
}

/// Every measured finding: token contrast, type scale, weights and faces, the palette rule and
/// the ramps.
fn findings(style: &ResolvedStyle) -> Vec<String> {
    let mut out: Vec<String> = style_checks::style_findings(style)
        .into_iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    out.extend(
        style_checks::palette(style)
            .into_iter()
            .map(|i| format!("palette {i}")),
    );
    out.extend(style_checks::ramps(style));
    out
}

fn hex(css: &str) -> String {
    richview_chart::color::parse(css).unwrap().hex()
}

fn blocking(diagnostics: &[richview_core::document::Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|d| d.level != "info")
        .map(|d| format!("{} {} {}", d.code, d.path, d.message))
        .collect()
}

const BRANDS: [&str; 8] = [
    "#0F62FE", "#F2C200", "#E30613", "#00A86B", "#6B6B6B", "#000000", "#FFFFFF", "#7B2FBF",
];
const BACKGROUNDS: [Option<&str>; 10] = [
    None,
    Some("#FFFFFF"),
    Some("#F7F3EA"),
    Some("#0B1630"),
    Some("#000000"),
    Some("#2D2A4A"),
    // Pages an earlier draft moved needlessly, and one mid-tone that must move.
    Some("#334155"),
    Some("#D8D8D8"),
    Some("#FFA500"),
    Some("#808080"),
];

#[test]
fn there_are_ten_presets_and_each_resolves_cleanly_and_passes_every_check() {
    assert_eq!(builtin_style_names().len(), 5);
    assert_eq!(preset_names().len(), 10);
    let mut failures = vec![];
    for name in preset_names() {
        let (style, diagnostics) = styled(&[name]);
        failures.extend(
            blocking(&diagnostics)
                .into_iter()
                .map(|d| format!("{name}: {d}")),
        );
        failures.extend(findings(&style).into_iter().map(|f| format!("{name}: {f}")));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn an_extended_preset_is_its_style_file_and_a_file_can_start_from_it() {
    let (by_name, _) = styled(&["midnight"]);
    let (by_file, diagnostics) = styled(&["preset: midnight\n"]);
    assert!(blocking(&diagnostics).is_empty(), "{diagnostics:?}");
    assert_eq!(by_name.css, by_file.css);
    assert_eq!(by_name.scheme, "dark");
    assert_eq!(hex(&by_name.custom_properties["--rv-bg"]), "#0b1630");
    // An author's own colour replaces the preset's brand and keeps its page.
    let (mine, _) = styled(&["preset: midnight\ncolors:\n  brand: '#E30613'\n"]);
    assert_eq!(
        mine.custom_properties["--rv-bg"],
        by_name.custom_properties["--rv-bg"]
    );
    assert_ne!(mine.chart["palette"][0], by_name.chart["palette"][0]);
    for name in EXTENDED_PRESETS {
        let (style, _) = styled(&[name]);
        assert!(
            style.chart["palette"]
                .as_array()
                .is_some_and(|p| p.len() == 6),
            "{name}"
        );
    }
}

#[test]
fn medium_is_the_presets_own_value_so_leaving_a_setting_out_changes_nothing() {
    let all_medium: String = STEP_SETTINGS
        .iter()
        .map(|setting| format!("{}: medium\n", setting.name))
        .collect();
    for name in builtin_style_names() {
        let (plain, _) = styled(&[name]);
        let (medium, diagnostics) = styled(&[&format!("preset: {name}\n{all_medium}")]);
        assert!(diagnostics.is_empty(), "{name}: {diagnostics:?}");
        assert_eq!(plain.css, medium.css, "{name}");
        assert_eq!(plain.chart, medium.chart, "{name}");
    }
}

#[test]
fn retired_company_colours_cannot_select_air_or_change_the_palette() {
    let old = "company:\n  color: '#0B6E69'\n  secondary: '#C2650F'\n  base: air\n";
    let new = "preset: air\ncolors:\n  brand: '#0B6E69'\n  secondary: '#C2650F'\n";
    let (old_style, old_diagnostics) = styled(&[old]);
    let (new_style, new_diagnostics) = styled(&[new]);
    assert!(new_diagnostics.is_empty(), "{new_diagnostics:?}");
    assert_eq!(old_style.document, styled(&["sleek"]).0.document);
    assert_eq!(hex(&new_style.custom_properties["--rv-accent"]), "#0b6e69");
    let legacy: Vec<_> = old_diagnostics
        .iter()
        .filter(|d| d.code == "style.retired")
        .map(|d| (d.path.as_str(), d.level.as_str()))
        .collect();
    assert_eq!(
        legacy,
        [
            ("styles[0].company.base", "warning"),
            ("styles[0].company.color", "warning"),
            ("styles[0].company.secondary", "warning"),
        ]
    );
}

#[test]
fn a_brand_colour_that_meets_its_floors_is_kept_to_the_last_bit() {
    for (preset, brand) in [
        ("air", "#0F62FE"),
        ("scientific", "#8A2432"),
        ("sleek", "#F2B544"),
    ] {
        let (style, _) = styled(&[&format!("preset: {preset}\ncolors:\n  brand: '{brand}'\n")]);
        assert_eq!(
            hex(&style.custom_properties["--rv-accent"]),
            brand.to_lowercase(),
            "{preset}"
        );
        assert_eq!(style.chart["palette"][0], brand.to_lowercase(), "{preset}");
    }
}

#[test]
fn a_brand_colour_with_an_alpha_is_refused() {
    for colour in [
        "rgb(1, 2, 3, 0.5)",
        "hsl(10, 50%, 40%, 0.5)",
        "#0F62FE80",
        "rgba(1, 2, 3, 1)",
    ] {
        let (_, diagnostics) = styled(&[&format!("colors:\n  brand: '{colour}'\n")]);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.path == "styles[0].colors.brand"),
            "{colour}: {diagnostics:?}"
        );
    }
}

#[test]
fn every_retired_leaf_is_skipped_with_one_located_warning() {
    use richview_core::{StyleMark, style_vocabulary};
    let baseline = styled(&["air", "spacing: large\n"]).0;
    for leaf in style_vocabulary()
        .iter()
        .filter(|leaf| leaf.mark == StyleMark::Legacy)
    {
        let parts: Vec<_> = leaf.path.split('.').collect();
        // Presets supply real values for their internal leaves. The former shortcuts are
        // author-only and have no entry in a resolved preset.
        let example = match leaf.path.as_str() {
            "density" => serde_json::json!("airmax"),
            "block-gap" => serde_json::json!("wide"),
            "type-size" => serde_json::json!("small"),
            "accent" | "company.color" | "company.secondary" => serde_json::json!("#001122"),
            "company.base" => serde_json::json!("brutalist"),
            _ => builtin_style_names()
                .iter()
                .find_map(|name| {
                    styled(&[name])
                        .0
                        .document
                        .pointer(&format!("/{}", leaf.path.replace('.', "/")))
                        .cloned()
                })
                .unwrap_or(serde_json::json!(42)),
        };
        for value in [example, serde_json::Value::Null] {
            let mut source = String::from("spacing: large\n");
            for (depth, key) in parts.iter().enumerate() {
                source.push_str(&"  ".repeat(depth));
                source.push_str(key);
                source.push(':');
                if depth + 1 == parts.len() {
                    source.push(' ');
                    source.push_str(&value.to_string());
                }
                source.push('\n');
            }
            let (style, warnings) = styled(&["air", &source]);
            assert_eq!(style.document, baseline.document, "{}", leaf.path);
            assert_eq!(warnings.len(), 1, "{source}: {warnings:?}");
            let d = &warnings[0];
            assert_eq!(
                (d.level.as_str(), d.code.as_str()),
                ("warning", "style.retired")
            );
            assert_eq!(d.path, format!("styles[1].{}", leaf.path));
            assert_eq!(d.line, Some(parts.len() + 1));
            assert_eq!(d.column, Some(1 + (parts.len() - 1) * 2));
            assert!(d.fix.is_some());
        }
    }
    for (word, replacement) in [("on", "medium"), ("off", "xsmall")] {
        let (style, warnings) = styled(&["air", &format!("spacing: large\nmotion: {word}\n")]);
        assert_eq!(style.document, baseline.document);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, "style.retired");
        assert_eq!(warnings[0].line, Some(2));
        assert!(
            warnings[0]
                .message
                .contains(&format!("motion: {replacement}"))
        );
    }
}

#[test]
fn retired_warnings_give_the_supported_spelling_or_leave_the_choice_to_the_preset() {
    for (source, expected) in [
        (
            "chart: {palette: air}",
            "Retired style leaf 'chart.palette' is ignored. Use `palette` instead.",
        ),
        (
            "motion: off",
            "Retired style leaf 'motion' is ignored. Use `motion: xsmall` instead.",
        ),
        (
            "tokens: {line-base: 1.65}",
            "Retired style leaf 'tokens.line-base' is ignored. Remove it. The preset controls this value.",
        ),
    ] {
        let (_, warnings) = styled(&["air", source]);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, expected);
    }
}

#[test]
fn retired_leaves_do_not_override_supported_steps_or_colours() {
    for colors in ["", "colors: {brand: '#0F62FE'}\n"] {
        let baseline = format!("preset: air\n{colors}corners: xlarge\ntint: large\n");
        let mixed = format!("{baseline}tokens: {{radius-m: 4px, bg: '#FAFAFA'}}\n");
        let (style, warnings) = styled(&[&mixed]);
        assert_eq!(style.document, styled(&[&baseline]).0.document);
        assert_eq!(
            warnings
                .iter()
                .filter(|d| d.code == "style.retired")
                .count(),
            2
        );
    }
}

#[test]
fn a_page_colour_moves_only_when_text_or_series_need_it() {
    use richview_chart::color;
    for (page, kept) in [
        ("#334155", true),
        ("#D8D8D8", true),
        ("#FFA500", true),
        ("#FFFFFF", true),
        ("#0B1630", true),
        ("#808080", false),
        ("#D01FE6", false),
    ] {
        for preset in ["air", "sleek", "scientific"] {
            let source =
                format!("preset: {preset}\ncolors:\n  brand: '#0F62FE'\n  background: '{page}'\n");
            let (style, diagnostics) = styled(&[&source]);
            let bg = hex(&style.custom_properties["--rv-bg"]);
            let moved = diagnostics
                .iter()
                .any(|d| d.path == "styles[0].colors.background");
            assert_eq!(
                bg == page.to_lowercase(),
                kept,
                "{preset} {page}: page is {bg}"
            );
            assert_eq!(moved, !kept, "{preset} {page}: {diagnostics:?}");
            assert!(
                findings(&style).is_empty(),
                "{preset} {page}: {:?}",
                findings(&style)
            );
            if !kept {
                // Only as far as needed: well short of where an earlier draft moved it.
                let l = color::parse(&style.custom_properties["--rv-bg"]).unwrap().l;
                assert!(l < 0.88 && l > 0.32, "{preset} {page}: moved to L {l}");
            }
        }
    }
}

#[test]
fn padding_reaches_a_framed_chart_and_lines_reach_the_metric_rules() {
    let css = |preset: &str, source: &str| styled(&[preset, source]).0.css;
    assert!(css("sleek", "padding: xlarge\n").contains(".rv-chart{padding:1.5rem 1.875rem}"));
    assert!(css("brutalist", "padding: xsmall\n").contains(".rv-chart{padding:0.75rem 0.938rem}"));
    assert!(css("air", "padding: large\n").contains(".rv-chart{padding-top:0.938rem}"));
    assert!(
        css("sleek", "lines: xlarge\n")
            .contains(".rv-metric{border-top-width:2px;border-bottom-width:2px}")
    );
    assert!(css("brutalist", "lines: xlarge\n").contains(".rv-metric{border-width:4px}"));
    // At medium nothing is added.
    assert_eq!(
        css("sleek", "padding: medium\nlines: medium\n"),
        styled(&["sleek"]).0.css
    );
}

fn px(style: &ResolvedStyle, key: &str) -> f64 {
    let value = &style.custom_properties[key];
    value
        .trim_end_matches("px")
        .trim_end_matches("rem")
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("{key}: {value}"))
}

#[test]
fn each_step_moves_its_setting_one_way_and_keeps_the_type_scale_ratios() {
    for name in builtin_style_names() {
        let at = |source: &str| styled(&[name, source]).0;
        let sizes: Vec<_> = STEPS
            .iter()
            .map(|step| at(&format!("text-size: {step}\n")))
            .collect();
        for key in ["--rv-size-base", "--rv-size-display", "--rv-block-gap"] {
            let values: Vec<f64> = sizes.iter().map(|style| px(style, key)).collect();
            assert!(
                values.windows(2).all(|w| w[0] < w[1]),
                "{name} {key}: {values:?}"
            );
        }
        // Phones move at most a tenth either way.
        let phone: Vec<f64> = sizes
            .iter()
            .map(|s| px(s, "--rv-size-base-phone"))
            .collect();
        assert!(
            (phone[0] / phone[2] - 0.9).abs() < 0.01 && (phone[4] / phone[2] - 1.1).abs() < 0.01
        );
        let gaps: Vec<f64> = STEPS
            .iter()
            .map(|step| px(&at(&format!("spacing: {step}\n")), "--rv-block-gap"))
            .collect();
        assert!(
            gaps.windows(2).all(|w| w[0] < w[1]),
            "{name} spacing: {gaps:?}"
        );
        assert!(gaps[0] >= 1.25, "{name}: {gaps:?}");
        let density: Vec<f64> = STEPS
            .iter()
            .map(|step| px(&at(&format!("padding: {step}\n")), "--rv-density"))
            .collect();
        assert!(
            density.windows(2).all(|w| w[0] < w[1]),
            "{name} padding: {density:?}"
        );
        let corners: Vec<f64> = STEPS
            .iter()
            .map(|step| px(&at(&format!("corners: {step}\n")), "--rv-radius-m"))
            .collect();
        assert!(
            corners.windows(2).all(|w| w[0] <= w[1]),
            "{name} corners: {corners:?}"
        );
        assert!(
            corners[0] == 0.0 && corners[4] > corners[2],
            "{name} corners: {corners:?}"
        );
        let still = at("motion: xsmall\n");
        assert_eq!(still.document["motion"]["enabled"], false, "{name}");
        let lively = at("motion: xlarge\n");
        let base = at("motion: medium\n");
        assert!(
            lively.document["motion"]["duration"].as_f64()
                > base.document["motion"]["duration"].as_f64(),
            "{name}"
        );
        assert!(lively.document["motion"]["duration"].as_f64().unwrap() <= 1000.0);
    }
}

#[test]
fn gridlines_and_tint_move_only_colour_and_never_below_a_floor() {
    use richview_chart::color;
    for name in preset_names() {
        let at = |source: &str| styled(&[name, source]).0;
        let medium = at("gridlines: medium\n");
        let chart_surface = |style: &ResolvedStyle| {
            color::parse(&style.custom_properties["--rv-bg-chart"]).unwrap()
        };
        let grid: Vec<f64> = STEPS
            .iter()
            .map(|step| {
                let style = at(&format!("gridlines: {step}\n"));
                // The page's own light separators keep their colour.
                assert_eq!(
                    style.custom_properties["--rv-rule-soft"],
                    medium.custom_properties["--rv-rule-soft"],
                    "{name} {step}"
                );
                let soft = style
                    .custom_properties
                    .get("--rv-grid")
                    .unwrap_or(&style.custom_properties["--rv-rule-soft"]);
                color::contrast(color::parse(soft).unwrap(), chart_surface(&style))
            })
            .collect();
        assert!(grid[0] >= 1.035, "{name}: never invisible: {grid:?}");
        assert!(
            grid.windows(2).all(|w| w[0] <= w[1] + 1e-6),
            "{name}: {grid:?}"
        );
        assert!(
            !medium.custom_properties.contains_key("--rv-grid"),
            "{name}"
        );
        let grey = at("tint: xsmall\ncolors:\n  brand: '#0F62FE'\n");
        let ink = color::parse(&grey.custom_properties["--rv-ink"]).unwrap();
        assert!(ink.c < 0.005, "{name}: {ink:?}");
        for step in STEPS {
            let style = at(&format!("tint: {step}\ncolors:\n  brand: '#0F62FE'\n"));
            assert!(
                findings(&style).is_empty(),
                "{name} tint {step}: {:?}",
                findings(&style)
            );
        }
    }
}

/// The extremes of every setting at once, and alternating.
fn extreme_files() -> Vec<String> {
    let all = |step: &str| -> String {
        STEP_SETTINGS
            .iter()
            .map(|setting| format!("{}: {step}\n", setting.name))
            .collect()
    };
    let alternating = |first: usize| -> String {
        STEP_SETTINGS
            .iter()
            .enumerate()
            .map(|(i, setting)| {
                let step = if (i + first).is_multiple_of(2) {
                    "xsmall"
                } else {
                    "xlarge"
                };
                format!("{}: {step}\n", setting.name)
            })
            .collect()
    };
    vec![all("xsmall"), all("xlarge"), alternating(0), alternating(1)]
}

#[test]
fn every_preset_holds_every_check_at_the_extremes_with_any_colours() {
    let mut failures = vec![];
    for name in preset_names() {
        for steps in extreme_files() {
            for brand in BRANDS {
                for background in BACKGROUNDS {
                    let background =
                        background.map_or(String::new(), |bg| format!("  background: '{bg}'\n"));
                    let source = format!(
                        "preset: {name}\n{steps}colors:\n  brand: '{brand}'\n  secondary: '#C2650F'\n{background}"
                    );
                    let (style, diagnostics) = styled(&[&source]);
                    for problem in blocking(&diagnostics).into_iter().chain(findings(&style)) {
                        failures.push(format!(
                            "{name} {brand} {background:?} [{}]: {problem}",
                            steps.replace('\n', " ")
                        ));
                    }
                }
            }
        }
    }
    let total = failures.len();
    failures.truncate(30);
    assert!(
        total == 0,
        "{total} failures, first:\n{}",
        failures.join("\n")
    );
}

#[test]
fn a_thousand_random_colours_hold_the_floors_on_every_main_preset() {
    // A fixed linear congruential generator: the same thousand colours every run.
    let mut seed: u64 = 0x5E_ED0F_C010;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 40) as u32 & 0xFF_FFFF
    };
    let mut failures = vec![];
    for index in 0..1000 {
        let brand = format!("#{:06X}", next());
        let background = (index % 3 != 0).then(|| format!("#{:06X}", next()));
        let name = builtin_style_names()[index % 5];
        let background = background.map_or(String::new(), |bg| format!("  background: '{bg}'\n"));
        let source = format!("preset: {name}\ncolors:\n  brand: '{brand}'\n{background}");
        let (style, diagnostics) = styled(&[&source]);
        for problem in blocking(&diagnostics).into_iter().chain(findings(&style)) {
            failures.push(format!("{name} {brand} {background:?}: {problem}"));
        }
    }
    let total = failures.len();
    failures.truncate(30);
    assert!(
        total == 0,
        "{total} failures, first:\n{}",
        failures.join("\n")
    );
}

/// Printing is checked in `src/style.rs`
/// (`a_dark_page_and_an_inline_extended_preset_print_light_and_keep_their_brand`).
#[test]
fn a_dark_page_colour_makes_a_dark_report() {
    let (style, _) =
        styled(&["preset: air\ncolors:\n  brand: '#0F62FE'\n  background: '#0B1630'\n"]);
    assert_eq!(style.scheme, "dark");
    assert_eq!(hex(&style.custom_properties["--rv-bg"]), "#0b1630");
    let (light, _) = styled(&["preset: sleek\ncolors:\n  background: '#FFFFFF'\n"]);
    assert_eq!(light.scheme, "light");
    assert!(findings(&light).is_empty(), "{:?}", findings(&light));
}

#[test]
fn a_bad_step_or_preset_is_skipped_with_a_finding() {
    let (style, diagnostics) = styled(&["text-size: huge\npreset: neon\ncolors: '#0F62FE'\n"]);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.code.as_str(), d.path.as_str()))
        .collect();
    assert!(
        codes.contains(&("style.schema", "styles[0].text-size")),
        "{codes:?}"
    );
    assert!(
        codes.contains(&("style.schema", "styles[0].preset")),
        "{codes:?}"
    );
    assert!(
        codes.contains(&("style.schema", "styles[0].colors")),
        "{codes:?}"
    );
    assert_eq!(style.css, styled(&["sleek"]).0.css);
}

#[test]
fn a_knockout_metric_keeps_its_text_readable_with_any_colours() {
    use richview_chart::color;
    let mut failures = vec![];
    for brand in BRANDS {
        for background in BACKGROUNDS {
            let background =
                background.map_or(String::new(), |bg| format!("  background: '{bg}'\n"));
            let source = format!("preset: brutalist\ncolors:\n  brand: '{brand}'\n{background}");
            let (style, _) = styled(&[&source]);
            let accent = color::parse(&style.custom_properties["--rv-accent"]).unwrap();
            let text = color::parse(&style.custom_properties["--rv-on-accent"]).unwrap();
            let ratio = color::contrast(accent, text);
            if ratio < 4.5 {
                failures.push(format!(
                    "{brand} {background:?}: text on the accent is {ratio:.2}:1"
                ));
            }
            failures.extend(
                findings(&style)
                    .into_iter()
                    .map(|f| format!("{brand}: {f}")),
            );
        }
    }
    // Brutalist alone keeps its white knockout text and its CSS as before.
    let (own, _) = styled(&["brutalist"]);
    assert!(!own.custom_properties.contains_key("--rv-on-accent"));
    assert!(own.css.contains("1.25rem;color:oklch(1 0 0)}"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The CSS without STY-74's generated background block.
fn without_background(css: &str) -> String {
    // The screen block, then the rule that drops it for high contrast and forced colours.
    let mut out = css.to_owned();
    for (start, end) in [
        ("@media screen{html{background:", "}}\n"),
        (
            "@media (prefers-contrast:more),(forced-colors:active){html{background:var(--rv-bg)!important}",
            "\n",
        ),
    ] {
        if let Some(at) = out.find(start) {
            let stop = out[at..]
                .find(end)
                .map_or(out.len(), |e| at + e + end.len());
            out.replace_range(at..stop, "");
        }
    }
    out
}

/// The builtin styles keep their pre-STY-73 output, apart from the shipped backgrounds.
#[test]
fn presets_resolve_byte_for_byte_as_before() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/style-dev");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    names.sort();
    assert_eq!(names.len(), builtin_style_names().len(), "{names:?}");
    let mut failures = vec![];
    for path in names {
        let fixture: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let refs: Vec<&str> = fixture["refs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        let (style, _) = styled(&refs);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // Three presets now ship a background (STY-74): its generated block is the one addition;
        // everything else, the custom properties included, is as before.
        if without_background(&style.css) != fixture["css"].as_str().unwrap() {
            failures.push(format!("{name}: css differs"));
        }
        if serde_json::to_value(&style.custom_properties).unwrap() != fixture["custom_properties"] {
            failures.push(format!("{name}: custom properties differ"));
        }
        if style.chart != fixture["chart"] {
            failures.push(format!("{name}: chart differs"));
        }
        if style.scheme != fixture["scheme"].as_str().unwrap() {
            failures.push(format!("{name}: scheme differs"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
