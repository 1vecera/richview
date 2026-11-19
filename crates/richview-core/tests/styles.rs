//! The five builtins, their names, the cascade under them and their print counterparts
//! (STY-05, STY-06, STY-07, STY-17, STY-18, STY-20, STY-21, STY-57, TYP-52).

use richview_core::{builtin_style_names, resolve_styles};

const TOKENS: [&str; 31] = [
    "bg",
    "bg-elev",
    "bg-chart",
    "ink",
    "ink-soft",
    "ink-faint",
    "chart-ink",
    "rule",
    "rule-soft",
    "accent",
    "accent-soft",
    "link",
    "brand",
    "up",
    "down",
    "hold",
    "font-display",
    "font-text",
    "font-ui",
    "font-mono",
    "line-base",
    "measure",
    "radius-s",
    "radius-m",
    "shadow-float",
    "scheme",
    "size-small",
    "size-base",
    "size-h3",
    "size-h2",
    "size-h1",
];

fn styled(
    refs: &[&str],
) -> (
    richview_core::ResolvedStyle,
    Vec<richview_core::document::Diagnostic>,
) {
    let mut diagnostics = vec![];
    let refs: Vec<String> = refs.iter().map(|r| r.to_string()).collect();
    let style = resolve_styles(&refs, &mut diagnostics);
    // These tests pin the old vocabulary, which still applies exactly; its `style.legacy`
    // warnings, which name what replaces each leaf, are tested in `style_presets.rs` (STY-73).
    diagnostics.retain(|d| d.code != "style.legacy");
    (style, diagnostics)
}

#[test]
fn there_are_five_builtins_and_each_one_sets_every_token() {
    assert_eq!(
        builtin_style_names(),
        ["sleek", "air", "scientific", "brutalist", "hand-drawn"]
    );
    for name in builtin_style_names() {
        let (style, diagnostics) = styled(&[name]);
        assert!(diagnostics.is_empty(), "{name}: {diagnostics:?}");
        let tokens = &style.document["tokens"];
        let mut missing: Vec<String> = TOKENS
            .iter()
            .filter(|key| tokens[**key].is_null())
            .map(|key| key.to_string())
            .collect();
        for step in ["small", "base", "h3", "h2", "h1", "display"] {
            if tokens[format!("size-{step}-phone")].is_null() {
                missing.push(format!("size-{step}-phone"));
            }
        }
        if tokens["size-display"].is_null() {
            missing.push("size-display".into());
        }
        if style.document["layout"]["report-max-width"].is_null() {
            missing.push("report-max-width".into());
        }
        assert!(missing.is_empty(), "{name} lacks {missing:?}");
        // A token that left the vocabulary stays out of every builtin.
        for gone in ["neutral", "accent-2"] {
            assert!(
                tokens[gone].is_null(),
                "{name} sets the retired token {gone}"
            );
        }
    }
}

#[test]
fn retired_names_are_errors_that_name_their_replacement() {
    for (old, new) in [
        ("sleek-dark", "sleek"),
        ("sleek-light", "air"),
        ("editorial", "brutalist"),
    ] {
        let (_, diagnostics) = styled(&[old]);
        let d = diagnostics
            .iter()
            .find(|d| d.code == "style.renamed")
            .unwrap_or_else(|| panic!("{old}: {diagnostics:?}"));
        assert_eq!(d.level, "error");
        assert!(d.message.contains(&format!("'{new}'")), "{}", d.message);
    }
    let (_, diagnostics) = styled(&["nonesuch"]);
    let d = diagnostics
        .iter()
        .find(|d| d.code == "style.unknown")
        .unwrap();
    assert_eq!(d.level, "error");
    assert!(
        d.message
            .contains("sleek, air, scientific, brutalist, hand-drawn")
    );
}

#[test]
fn a_cascade_of_files_starts_from_sleek_without_renumbering_them() {
    let file = "tokens:\n  accent: 'oklch(0.6 0.2 30)'\n";
    let (style, diagnostics) = styled(&[file]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(style.document["tokens"]["accent"], "oklch(0.6 0.2 30)");
    assert_eq!(
        style.document["tokens"]["bg"],
        styled(&["sleek"]).0.document["tokens"]["bg"]
    );
    // A bad leaf reports the file's own index, not one shifted by the builtin below it.
    let (_, diagnostics) = styled(&["tokens:\n  accent: notacolor\n"]);
    assert_eq!(diagnostics[0].path, "styles[0].tokens.accent");
    // Naming a builtin keeps the cascade as written.
    let (style, _) = styled(&["scientific", file]);
    assert_eq!(style.scheme, "light");
}

#[test]
fn each_builtin_declares_its_scheme_and_dark_ones_print_as_a_light_builtin() {
    for (name, scheme) in [
        ("sleek", "dark"),
        ("air", "light"),
        ("scientific", "light"),
        ("brutalist", "light"),
        ("hand-drawn", "light"),
    ] {
        let (style, _) = styled(&[name]);
        assert_eq!(style.scheme, scheme, "{name}");
        assert!(
            style
                .css
                .contains(&format!(":root{{color-scheme:{scheme}}}")),
            "{name}"
        );
        assert!(!style.custom_properties.contains_key("--rv-scheme"));
        assert!(!style.custom_properties.contains_key("--rv-print-as"));
    }
}

#[test]
fn the_base_stylesheet_has_no_colours_faces_sizes_or_scheme_rules() {
    let css = include_str!("../assets/report.css");
    for forbidden in ["prefers-color-scheme", "data-theme", "oklch(", "#"] {
        assert!(
            !css.lines()
                .filter(|l| !l.trim_start().starts_with("/*"))
                .any(|l| l.contains(forbidden) && !l.contains("var(--rv-")),
            "report.css contains {forbidden}"
        );
    }
    assert!(
        !css.contains("--rv-size-small:"),
        "sizes come from the style"
    );
}

fn file(
    source: &str,
) -> (
    richview_core::ResolvedStyle,
    Vec<richview_core::document::Diagnostic>,
) {
    styled(&["sleek", source])
}

#[test]
fn a_bad_leaf_is_skipped_and_the_rest_of_the_file_applies() {
    let (style, diagnostics) = file(
        "tokens:\n  accent: notacolor\n  bg: 'oklch(0.3 0.01 200)'\n  nope: 1\nlayout:\n  report-max-width: 1200px\n",
    );
    assert_eq!(style.document["tokens"]["bg"], "oklch(0.3 0.01 200)");
    assert_eq!(
        style.document["tokens"]["accent"],
        styled(&["sleek"]).0.document["tokens"]["accent"]
    );
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.code.as_str(), d.path.as_str()))
        .collect();
    assert_eq!(
        codes,
        [
            ("style.internal", "styles[1].layout.report-max-width"),
            ("style.schema", "styles[1].tokens.accent"),
            ("style.unknown-key", "styles[1].tokens.nope"),
        ]
    );
    assert_eq!(style.document["layout"]["report-max-width"], "880px");
}

#[test]
fn settings_expand_into_the_sections_behind_them() {
    let (style, diagnostics) = file(
        "density: loose\nblock-gap: tight\npalette: air\nmotion: off\ncolophon: hide\nfonts:\n  text: 'Newsreader, Georgia, serif'\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let doc = &style.document;
    assert_eq!(doc["tokens"]["density"], 1.25);
    assert_eq!(doc["tokens"]["block-gap"], "3rem");
    assert_eq!(doc["tokens"]["block-gap-phone"], "2rem");
    assert_eq!(doc["chart"]["palette"], "air");
    assert_eq!(doc["motion"]["enabled"], false);
    assert_eq!(doc["colophon"], "hide");
    assert_eq!(doc["tokens"]["font-text"], "Newsreader, Georgia, serif");
    assert_eq!(style.custom_properties["--rv-block-gap"], "3rem");
    assert!(!style.custom_properties.contains_key("--rv-settings"));
}

#[test]
fn type_size_scales_the_whole_scale_and_keeps_its_steps() {
    let base = styled(&["sleek"]).0;
    let (small, _) = file("type-size: small\n");
    let px = |style: &richview_core::ResolvedStyle, step: &str| {
        style.custom_properties[&format!("--rv-size-{step}")]
            .trim_end_matches("px")
            .parse::<f64>()
            .unwrap()
    };
    for step in ["small", "base", "h3", "h2", "h1", "display"] {
        let ratio = px(&small, step) / px(&base, step);
        assert!((ratio - 0.9).abs() < 0.01, "{step}: {ratio}");
    }
}

#[test]
fn the_accent_setting_moves_the_accent_links_and_first_series_together() {
    let (style, diagnostics) = styled(&["air", "accent: 'oklch(0.4128 0.1233 255.65)'\n"]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let tokens = &style.document["tokens"];
    assert_eq!(tokens["accent"], "oklch(0.4128 0.1233 255.65)");
    assert_eq!(tokens["link"], "oklch(0.4128 0.1233 255.65)");
    assert_eq!(
        style.document["chart"]["palette"][0],
        "oklch(0.4128 0.1233 255.65)"
    );
    // On Sleek's dark page the deep blue is too weak to read as text: links keep their colour.
    let (sleek, _) = styled(&["sleek", "accent: 'oklch(0.4128 0.1233 255.65)'\n"]);
    assert_eq!(
        sleek.document["tokens"]["link"],
        styled(&["sleek"]).0.document["tokens"]["link"]
    );
    // Hand-drawn's first series is graphite, not its red pencil, so the accent leaves it alone.
    let (drawn, _) = styled(&["hand-drawn", "accent: 'oklch(0.4128 0.1233 255.65)'\n"]);
    assert_eq!(
        drawn.document["chart"]["palette"],
        styled(&["hand-drawn"]).0.document["chart"]["palette"]
    );
}

#[test]
fn the_vocabulary_marks_every_leaf_and_no_taught_setting_is_experimental() {
    let vocabulary = richview_core::style_vocabulary();
    let mark = |path: &str| {
        vocabulary
            .iter()
            .find(|leaf| leaf.path == path)
            .unwrap()
            .mark
    };
    // Tokens are a preset's internal vocabulary: they still apply, and warn (STY-73).
    assert_eq!(mark("tokens.accent"), richview_core::StyleMark::Legacy);
    assert_eq!(mark("tokens.line-base"), richview_core::StyleMark::Legacy);
    assert_eq!(
        mark("components.metric.style"),
        richview_core::StyleMark::Legacy
    );
    assert_eq!(mark("tokens.measure"), richview_core::StyleMark::Internal);
    for path in [
        "preset",
        "colors.brand",
        "text-size",
        "corners",
        "motion",
        "palette",
    ] {
        assert_eq!(mark(path), richview_core::StyleMark::Safe, "{path}");
    }
    // Every combination of the taught settings is tested, so none is experimental.
    assert!(
        vocabulary
            .iter()
            .all(|leaf| leaf.mark != richview_core::StyleMark::Experimental),
    );
    assert!(vocabulary.iter().any(|leaf| leaf.path == "colophon"));
}

#[test]
fn brand_fonts_name_a_woff2_file_inside_the_report_folder() {
    let (style, diagnostics) =
        file("fonts:\n  text: {family: 'Acme Sans', file: fonts/acme.woff2}\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(style.document["tokens"]["font-text"], "Acme Sans");
    assert_eq!(
        style.document["settings"]["brand-fonts"]["text"]["file"],
        "fonts/acme.woff2"
    );
    for bad in [
        "../x.woff2",
        "/etc/x.woff2",
        "fonts/a.ttf",
        "fonts//a.woff2",
    ] {
        let (_, diagnostics) = file(&format!("fonts:\n  text: {{family: A, file: '{bad}'}}\n"));
        assert!(
            diagnostics.iter().any(|d| d.code == "style.schema"),
            "{bad}"
        );
    }
}

#[test]
fn the_builtins_differ_in_structure_not_only_in_colour_and_type() {
    use richview_core::document::{RenderOptions, ReportFolder, render};
    let source = "---\ntitle: Claim\n---\n\n## Section\n\n```big\nvalue: 68.2\nunit: '%'\n```\n\n```callout\nkind: hold\ntitle: Read\nbody: Body.\n```\n";
    let mut structure = std::collections::BTreeSet::new();
    for name in builtin_style_names() {
        let html = render(
            source,
            &RenderOptions {
                styles: vec![name.to_string()],
                report_folder: Some(ReportFolder {
                    root: "fixture".into(),
                    source: "report.rmd".into(),
                    files: vec!["report.rmd".into()],
                }),
                ..Default::default()
            },
        )
        .unwrap()
        .html;
        // The generated component rules, with every colour and font reference removed.
        let mut rules: Vec<String> = html
            .split("}\n")
            .filter(|rule| rule.starts_with(".rv-") || rule.starts_with(".rv-chart"))
            .filter(|rule| {
                rule.contains("border") || rule.contains("order:") || rule.contains("background:")
            })
            .map(str::to_owned)
            .collect();
        rules.sort();
        structure.insert(rules.join("\n"));
    }
    assert_eq!(
        structure.len(),
        builtin_style_names().len(),
        "two builtins share one structure"
    );
}

#[test]
fn hand_drawn_marks_the_sign_of_heatmap_cells_with_a_hatch_and_the_other_builtins_do_not() {
    use richview_core::document::{RenderOptions, ReportFolder, render};
    let source = "```chart\ntype: heatmap\ntitle: Signs\nencoding: {x: c, y: r, color: v}\ndata:\n  inline:\n    - {c: A, r: Now, v: -40}\n    - {c: B, r: Now, v: 40}\n```\n";
    let hatched = |style: &str| {
        let report = render(
            source,
            &RenderOptions {
                styles: vec![style.into()],
                report_folder: Some(ReportFolder {
                    root: "fixture".into(),
                    source: "report.rmd".into(),
                    files: vec!["report.rmd".into()],
                }),
                ..Default::default()
            },
        )
        .unwrap();
        report.charts[0]["theme"]["signHatch"] == true
    };
    assert!(hatched("hand-drawn"), "negative cells carry a pencil hatch");
    for other in ["sleek", "air", "scientific", "brutalist"] {
        assert!(!hatched(other), "{other}");
    }
}

#[test]
fn only_hand_drawn_keeps_the_flat_glow_whatever_its_decals_switch_says() {
    use richview_core::document::{RenderOptions, ReportFolder, render};
    let flat = |refs: &[&str]| styled(refs).0.document["chart"]["glow"].as_str() == Some("flat");
    assert!(flat(&["hand-drawn"]));
    assert!(
        flat(&["hand-drawn", "chart:\n  decals: false\n"]),
        "Hand-drawn without decals stays flat"
    );
    assert!(
        flat(&["sleek", "preset: chalk\n"]),
        "Chalk is Hand-drawn on slate"
    );
    for other in ["sleek", "air", "scientific", "brutalist"] {
        assert!(!flat(&[other]), "{other}");
    }
    assert!(
        !flat(&["sleek", "chart:\n  decals: true\n"]),
        "Sleek with decals keeps the glow"
    );
    let (style, diagnostics) = file("chart:\n  glow: flat\n");
    assert!(
        style.document["chart"]["glow"].is_null(),
        "only builtins set the glow"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "style.internal" && d.path == "styles[1].chart.glow")
    );
    // The payload carries the flag only where it is set.
    let theme = |style: &str| {
        let report = render(
            "```chart\ntype: scatter\ntitle: T\nencoding: {x: a, y: b}\ndata:\n  inline:\n    - {a: 1, b: 2}\n```\n",
            &RenderOptions {
                styles: vec![style.into()],
                report_folder: Some(ReportFolder {
                    root: "fixture".into(),
                    source: "report.rmd".into(),
                    files: vec!["report.rmd".into()],
                }),
                ..Default::default()
            },
        )
        .unwrap();
        report.charts[0]["theme"].clone()
    };
    assert_eq!(theme("hand-drawn")["flatGlow"], true);
    assert!(theme("sleek").get("flatGlow").is_none());
}

#[test]
fn a_cascade_still_dark_after_the_print_swap_warns_and_builtins_never_do() {
    use richview_core::document::{RenderOptions, ReportFolder, render};
    let warns = |styles: Vec<String>| {
        render(
            "# T\n",
            &RenderOptions {
                styles,
                report_folder: Some(ReportFolder {
                    root: "fixture".into(),
                    source: "report.rmd".into(),
                    files: vec!["report.rmd".into()],
                }),
                ..Default::default()
            },
        )
        .unwrap()
        .diagnostics
        .iter()
        .any(|d| d.code == "style.print-dark")
    };
    for name in builtin_style_names() {
        assert!(!warns(vec![(*name).into()]), "{name}");
    }
    let darkened = "tokens:\n  bg: \"oklch(0.15 0 0)\"\n  ink: \"oklch(0.95 0 0)\"\n".to_owned();
    assert!(warns(vec!["air".into(), darkened]));
}

#[test]
fn each_builtin_names_the_structural_mark_of_its_link_preview_thumbnail() {
    let marks: Vec<String> = builtin_style_names()
        .iter()
        .map(|name| {
            let (style, _) = styled(&[name]);
            style.document["components"]["thumbnail"]["rule"]
                .as_str()
                .unwrap_or_else(|| panic!("{name} has no thumbnail rule"))
                .to_owned()
        })
        .collect();
    assert_eq!(marks, ["none", "none", "hairline", "ink-bars", "pencil"]);
}

#[test]
fn each_builtin_has_its_own_motion_character_and_publishes_it_as_properties() {
    let expected = [
        ("sleek", "gentle", "out"),
        ("air", "gentle", "soft"),
        ("scientific", "restrained", "standard"),
        ("brutalist", "crisp", "snap"),
        ("hand-drawn", "playful", "spring"),
    ];
    let mut durations = std::collections::BTreeSet::new();
    for (name, character, easing) in expected {
        let (style, diagnostics) = styled(&[name]);
        assert!(diagnostics.is_empty(), "{name}: {diagnostics:?}");
        let motion = &style.document["motion"];
        assert_eq!(motion["character"], character, "{name}");
        assert_eq!(motion["easing"], easing, "{name}");
        durations.insert(motion["duration"].as_u64().unwrap());
        assert!(style.css.contains("--rv-motion-duration:"), "{name}");
        assert!(style.css.contains("--rv-motion-block:"), "{name}");
    }
    assert_eq!(
        durations.len(),
        5,
        "each style times its entrances differently"
    );
    let (off, _) = styled(&["brutalist"]);
    assert!(off.css.contains("--rv-motion-block:none"));
}

#[test]
fn a_motion_character_preset_matches_the_builtin_that_names_it() {
    // `motion: {character: …}` alone fills the timing a builtin of that character sets in its file.
    for name in builtin_style_names() {
        let mut diagnostics = vec![];
        let builtin = resolve_styles(&[name.to_string()], &mut diagnostics);
        let motion = &builtin.document["motion"];
        let Some(character) = motion["character"].as_str() else {
            continue;
        };
        // Air is gentle with a voice of its own (640 ms, soft); it sets every leaf itself.
        if character == "still" || *name == "air" {
            continue;
        }
        let preset = resolve_styles(
            &[format!("motion: {{character: {character}}}")],
            &mut diagnostics,
        );
        for key in [
            "duration",
            "easing",
            "stagger",
            "block-entrance",
            "distance",
        ] {
            assert_eq!(
                preset.document["motion"][key], motion[key],
                "{name}: motion.{key} of character {character}"
            );
        }
    }
}

#[test]
fn corners_follow_the_card_per_builtin_and_bars_are_square_in_the_square_styles() {
    // STY-36: Sleek and Air 12 px, Scientific 2 px, Brutalist and Hand-drawn square; bar ends 3 px
    // except the square styles.
    let expected = [
        ("sleek", "12px", 3.0),
        ("air", "12px", 3.0),
        ("scientific", "2px", 3.0),
        ("brutalist", "0px", 0.0),
        ("hand-drawn", "0px", 0.0),
    ];
    for (name, radius, bars) in expected {
        let style = resolve_styles(&[name.to_string()], &mut vec![]);
        for token in ["radius-s", "radius-m"] {
            assert_eq!(style.document["tokens"][token], radius, "{name} {token}");
        }
        assert_eq!(
            style.chart["bar-radius"].as_f64(),
            Some(bars),
            "{name} bar-radius"
        );
    }
}

#[test]
fn brand_shortcut_validates_its_colours_and_builtin_base() {
    let (_, valid) =
        styled(&["company:\n  color: '#0B6E69'\n  secondary: '#C2650F'\n  base: air\n"]);
    assert!(valid.is_empty(), "{valid:?}");
    let (_, invalid) = styled(&[
        "company:\n  color: 'red; color: blue'\n  secondary: notacolor\n  base: missing\n",
    ]);
    for path in ["company.color", "company.secondary", "company.base"] {
        assert!(
            invalid
                .iter()
                .any(|d| d.code == "style.schema" && d.path.ends_with(path)),
            "{path}: {invalid:?}"
        );
    }
}

#[test]
fn brand_logo_paths_stay_inside_the_report_and_name_is_plain_text() {
    let (style, valid) = styled(&[
        "company:\n  name: 'Česká firma'\n  logo: 'assets/Logo.PNG'\n  logo-dark: 'assets/Logo Dark.webp'\n",
    ]);
    assert!(valid.is_empty(), "{valid:?}");
    assert_eq!(style.document["company"]["name"], "Česká firma");
    assert_eq!(style.document["company"]["logo"], "assets/Logo.PNG");
    for logo in [
        "../logo.png",
        "/logo.png",
        "https://example.com/logo.png",
        "data:image/png;base64,AA",
        "images/../../logo.webp",
    ] {
        let (_, diagnostics) = styled(&[&format!("company:\n  logo: '{logo}'\n")]);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "style.schema" && d.path.ends_with("company.logo")),
            "{logo}: {diagnostics:?}"
        );
    }
    let (_, path_only) = styled(&["company:\n  logo: 'assets/logo.svg'\n"]);
    assert!(
        path_only.is_empty(),
        "type is checked from bytes by the logo resolver: {path_only:?}"
    );
    let (_, invalid) = styled(&["company:\n  name: '  '\n"]);
    assert!(
        invalid
            .iter()
            .any(|d| d.code == "style.schema" && d.path.ends_with("company.name"))
    );
}

#[test]
fn brand_base_is_the_named_builtin_and_explicit_file_overrides_win() {
    let (air, _) = styled(&["air"]);
    let (branded, diagnostics) =
        styled(&["company:\n  base: air\ntokens:\n  font-display: Geist\n"]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(branded.scheme, "light");
    assert_eq!(
        branded.document["tokens"]["bg"],
        air.document["tokens"]["bg"]
    );
    assert_eq!(branded.document["tokens"]["font-display"], "Geist");
}

#[test]
fn brand_derives_accessible_palette_and_reports_necessary_adjustments() {
    use richview_core::style_checks;
    for (primary, secondary, base, should_adjust) in [
        ("#0B6E69", Some("#C2650F"), "air", false),
        ("#F2C200", None, "air", true),
        ("#0A1F44", None, "sleek", true),
        ("#6B6B6B", None, "air", false),
    ] {
        let secondary =
            secondary.map_or(String::new(), |value| format!("  secondary: '{value}'\n"));
        let source = format!("company:\n  color: '{primary}'\n  base: {base}\n{secondary}");
        let (style, diagnostics) = styled(&[&source]);
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.level == "warning" || d.level == "error"),
            "{primary}: {diagnostics:?}"
        );
        assert_eq!(
            diagnostics
                .iter()
                .any(|d| d.code == "style.company-adjusted"),
            should_adjust,
            "{primary}: {diagnostics:?}"
        );
        assert_eq!(style.chart["palette"].as_array().unwrap().len(), 6);
        assert!(
            style_checks::contrast(&style).is_empty(),
            "{primary}: {:?}",
            style_checks::contrast(&style)
        );
        assert!(
            style_checks::palette(&style).is_empty(),
            "{primary}: {:?}",
            style_checks::palette(&style)
        );
        if !should_adjust {
            assert_eq!(
                style.chart["palette"][0].as_str().unwrap(),
                primary.to_ascii_lowercase()
            );
        }
    }
}

#[test]
fn brand_derivation_respects_explicit_overrides_and_reports_an_indistinguishable_secondary() {
    let (style, diagnostics) = styled(&[
        "company:\n  color: '#0B6E69'\n  secondary: '#0B6E69'\n  base: air\npalette: ['#112233', '#aabbcc']\ntokens:\n  link: '#123456'\n",
    ]);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "style.company-secondary"),
        "{diagnostics:?}"
    );
    assert_eq!(
        style.chart["palette"],
        serde_json::json!(["#112233", "#aabbcc"])
    );
    assert_eq!(style.document["tokens"]["link"], "#123456");
}

#[test]
fn company_style_checks_hold_for_hostile_colours_and_preserve_richview_wordmark() {
    use richview_core::style_checks;
    for base in ["air", "sleek"] {
        let (original, _) = styled(&[base]);
        for color in [
            "#F2C200",
            "#39FF14",
            "#E30613",
            "#000000",
            "#6B6B6B",
            "#FFFFFF",
            "#0A1F44",
            "oklch(0.7 0.37 135)",
        ] {
            let source = format!("company:\n  color: '{color}'\n  base: {base}\n");
            let (derived, diagnostics) = styled(&[&source]);
            assert!(
                !diagnostics
                    .iter()
                    .any(|d| d.level == "warning" || d.level == "error"),
                "{base} {color}: {diagnostics:?}"
            );
            assert_eq!(
                derived.document["tokens"]["brand"], original.document["tokens"]["brand"],
                "{base} {color}"
            );
            assert!(
                style_checks::contrast(&derived).is_empty(),
                "{base} {color}: {:?}",
                style_checks::contrast(&derived)
            );
            assert!(
                style_checks::palette(&derived).is_empty(),
                "{base} {color}: {:?}",
                style_checks::palette(&derived)
            );
            assert!(
                style_checks::ramps(&derived).is_empty(),
                "{base} {color}: {:?}",
                style_checks::ramps(&derived)
            );
        }
    }
}

#[test]
fn company_rejects_translucent_and_unresolved_colours_with_a_finding() {
    for color in [
        "transparent",
        "currentcolor",
        "#1234",
        "#11223344",
        "rgba(1, 2, 3, 0.5)",
        "oklch(0.5 0.1 30 / 0.5)",
    ] {
        let source = format!("company:\n  color: '{color}'\n");
        let (style, diagnostics) = styled(&[&source]);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "style.company-color" && d.path.ends_with("company.color")),
            "{color}: {diagnostics:?}"
        );
        assert_eq!(
            style.scheme, "light",
            "invalid colour falls back to the default Air base"
        );
    }
}

#[test]
fn company_reports_when_only_the_link_needs_more_contrast() {
    let (style, diagnostics) = styled(&["company:\n  color: '#777777'\n  base: air\n"]);
    assert_eq!(style.chart["palette"][0], "#777777");
    let finding = diagnostics
        .iter()
        .find(|d| d.code == "style.company-adjusted")
        .expect("link adjustment is disclosed");
    assert!(finding.message.contains("#777777") && finding.message.contains("link"));
}

#[test]
fn company_uses_an_explicit_cascade_builtin_when_its_own_base_is_omitted() {
    let (base, _) = styled(&["brutalist"]);
    let (style, diagnostics) = styled(&["brutalist", "company:\n  color: '#0B6E69'\n"]);
    assert!(
        diagnostics
            .iter()
            .all(|d| d.level != "warning" && d.level != "error"),
        "{diagnostics:?}"
    );
    assert_eq!(
        style.document["tokens"]["font-display"],
        base.document["tokens"]["font-display"]
    );
    assert_eq!(
        style.document["components"]["title"],
        base.document["components"]["title"]
    );
}

#[test]
fn company_finds_a_checked_palette_for_rare_hues() {
    for color in ["#6E6076", "oklch(0.5 0.06 300)", "oklch(0.5 0.1 150)"] {
        let (style, diagnostics) =
            styled(&[&format!("company:\n  color: '{color}'\n  base: air\n")]);
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.level == "warning" || d.level == "error"),
            "{color}: {diagnostics:?}"
        );
        assert_eq!(style.chart["palette"].as_array().unwrap().len(), 6);
        assert!(
            richview_core::style_checks::palette(&style).is_empty(),
            "{color}"
        );
    }
}

#[test]
fn teal_company_palette_is_a_sibling_of_airs_designed_palette() {
    let (style, diagnostics) = styled(&["company:\n  color: '#0B6E69'\n  base: air\n"]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(
        style.chart["palette"],
        serde_json::json!([
            "#0b6e69", "#c9690c", "#7f62a0", "#50a06d", "#334182", "#9d3434"
        ])
    );
}

#[test]
fn yellow_company_palette_follows_the_reviewed_recipe() {
    let (style, diagnostics) = styled(&["company:\n  color: '#F2C200'\n  base: air\n"]);
    assert!(
        diagnostics.iter().all(|d| d.level == "info"),
        "{diagnostics:?}"
    );
    assert_eq!(
        style.chart["palette"],
        serde_json::json!([
            "#b18d00", "#174bd7", "#338c77", "#7f62a0", "#005474", "#b55239"
        ])
    );
}

#[test]
fn repeated_company_renders_keep_each_base_and_secondary_distinct() {
    let company = "company:\n  color: '#0B6E69'\n";
    let second = "company:\n  color: '#0B6E69'\n  secondary: '#C2650F'\n";
    let (air, _) = styled(&["air", company]);
    let (sleek, _) = styled(&["sleek", company]);
    let (with_second, _) = styled(&["air", second]);
    let (air_again, _) = styled(&["air", company]);
    assert_ne!(air.document["tokens"]["bg"], sleek.document["tokens"]["bg"]);
    assert_ne!(air.chart["palette"], with_second.chart["palette"]);
    assert_eq!(air.document["tokens"], air_again.document["tokens"]);
    assert_eq!(air.chart["palette"], air_again.chart["palette"]);
}
