//! Backgrounds behind a report (STY-74): every texture, gradient and picture keeps every ink's
//! STY-25 floor over its worst pixel, in every preset, light and dark; a report without a
//! background is byte for byte as before; and every bad leaf or picture is a finding.

use richview_chart::color::{self, Oklch};
use richview_core::document::{Output, RenderOptions, render};
use richview_core::{ResolvedStyle, builtin_style_names, resolve_styles};
use serde_json::Value;
use std::collections::BTreeMap;

/// The inks that can sit on a page background and their floors (STY-25): text, links, the
/// accent, ticks, and the status colours of deltas and callout titles. The wordmark's `brand`
/// is not drawn on a report's page.
const FLOORS: [(&str, f64); 8] = [
    ("ink", 7.0),
    ("ink-soft", 4.5),
    ("link", 4.5),
    ("accent", 3.0),
    ("ink-faint", 3.0),
    ("up", 4.5),
    ("down", 4.5),
    ("hold", 4.5),
];
const STEPS: [&str; 5] = ["xsmall", "small", "medium", "large", "xlarge"];

/// A 16x9 opaque PNG, a 4x4 PNG with an alpha channel, and a 2401x1 PNG.
const OK_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAABAAAAAJCAIAAAC0SDtlAAAAF0lEQVR42mOs6FnAQApgYiARjGqgiQYA4WwBtjxJERoAAAAASUVORK5CYII=";
const ALPHA_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAYAAACp8Z5+AAAAFUlEQVR42mM8oaERxYAEmBjQAGEBAGCkAXpxLlSGAAAAAElFTkSuQmCC";
const WIDE_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAACWEAAAABCAAAAACQ1KGgAAAAG0lEQVR42mNsYBgFo2AUjIJRMApGwSgYBVQFAMOAAILVKX59AAAAAElFTkSuQmCC";

/// The cascades the checks run over: every main preset, the dark extended presets, and the
/// author's colours light and dark.
fn bases() -> Vec<(String, String)> {
    let mut bases: Vec<(String, String)> = builtin_style_names()
        .iter()
        .map(|name| ((*name).to_owned(), format!("preset: {name}\n")))
        .collect();
    for name in ["midnight", "chalk"] {
        bases.push((name.into(), format!("preset: {name}\n")));
    }
    bases.push((
        "air + brand".into(),
        "preset: air\ncolors:\n  brand: \"#8E2C3A\"\n".into(),
    ));
    bases.push((
        "scientific + dark page".into(),
        "preset: scientific\ncolors:\n  brand: \"#E0A040\"\n  background: \"#0B1630\"\n".into(),
    ));
    bases
}

fn resolve(source: &str) -> (ResolvedStyle, Vec<richview_core::document::Diagnostic>) {
    let mut diagnostics = vec![];
    let style = resolve_styles(&[source.to_owned()], &mut diagnostics);
    (style, diagnostics)
}

fn token(style: &ResolvedStyle, name: &str) -> Oklch {
    style.document["tokens"][name]
        .as_str()
        .and_then(color::parse)
        .unwrap_or_else(|| panic!("token {name}"))
}

fn derived(style: &ResolvedStyle, key: &str) -> Option<Oklch> {
    style
        .document
        .pointer(&format!("/settings/background/derived/{key}"))
        .and_then(Value::as_str)
        .and_then(color::parse)
}

fn floors_hold(style: &ResolvedStyle, worst: &[Oklch], label: &str) {
    for (name, floor) in FLOORS {
        let ink = token(style, name);
        for pixel in worst {
            let ratio = color::contrast(ink, *pixel);
            assert!(
                ratio >= floor - 1e-9,
                "{label}: {name} reaches {ratio:.2}:1 over {} (floor {floor})",
                pixel.css()
            );
        }
    }
}

#[test]
fn every_texture_and_gradient_keeps_every_ink_floor_over_its_worst_pixel() {
    for (base, source) in bases() {
        for (texture, gradient) in [
            ("grain", "none"),
            ("dots", "none"),
            ("grid", "none"),
            ("lines", "none"),
            ("none", "soft"),
            ("none", "glow"),
            ("grain", "soft"),
            ("dots", "glow"),
        ] {
            for place in ["page", "header"] {
                for strength in STEPS {
                    let label = format!("{base} / {texture} + {gradient} / {place} / {strength}");
                    let file = format!(
                        "{source}background:\n  texture: {texture}\n  gradient: {gradient}\n  place: {place}\n  strength: {strength}\n"
                    );
                    let (style, diagnostics) = resolve(&file);
                    assert!(
                        diagnostics.iter().all(|d| d.level == "info"),
                        "{label}: {diagnostics:?}"
                    );
                    let page = token(&style, "bg");
                    let mut worst = vec![];
                    if texture != "none" {
                        let ink = derived(&style, "texture").expect("texture ink");
                        assert!(
                            color::contrast(ink, page) > 1.02,
                            "{label}: texture invisible"
                        );
                        worst.push(ink);
                    }
                    if gradient != "none" {
                        let stop = derived(&style, "stop").expect("gradient stop");
                        assert!(
                            color::contrast(stop, page) > 1.02,
                            "{label}: gradient invisible"
                        );
                        worst.extend((0..=32).map(|i| color::mix(page, stop, f64::from(i) / 32.0)));
                    }
                    floors_hold(&style, &worst, &label);
                    // The page itself still carries every ink.
                    floors_hold(&style, &[page], &label);
                }
            }
        }
    }
}

#[test]
fn grain_and_dots_are_present_at_medium_in_every_main_preset() {
    for name in builtin_style_names() {
        for (texture, least) in [("grain", 1.25), ("dots", 1.22)] {
            let (style, _) = resolve(&format!(
                "preset: {name}\nbackground:\n  texture: {texture}\n"
            ));
            let ratio = color::contrast(derived(&style, "texture").unwrap(), token(&style, "bg"));
            assert!(
                ratio >= least,
                "{name} / {texture}: {ratio:.2}:1 is too faint"
            );
        }
    }
}

#[test]
fn a_picture_under_text_is_washed_just_enough_for_any_pixel() {
    for (base, source) in bases() {
        let mut previous = 1.0;
        for strength in STEPS {
            let (style, _) = resolve(&format!(
                "{source}background:\n  picture: hero.png\n  place: page\n  strength: {strength}\n"
            ));
            let alpha = style
                .document
                .pointer("/settings/background/derived/wash")
                .and_then(Value::as_f64)
                .unwrap();
            assert!(
                alpha <= previous,
                "{base}: the wash thins as strength rises"
            );
            previous = alpha;
            let page = token(&style, "bg").srgb();
            let composites: Vec<Oklch> = [0.0, 1.0]
                .iter()
                .map(|extreme| Oklch::from_srgb(page.map(|p| alpha * p + (1.0 - alpha) * extreme)))
                .collect();
            floors_hold(
                &style,
                &composites,
                &format!("{base} / picture / {strength}"),
            );
        }
    }
}

/// The presets that ship a background, with the words they ship (STY-74 as amended, Daniel
/// 19 Nov: "at least three of the default styles ... should have backgrounds").
const DEFAULTS: [(&str, &str, Option<&str>, u64); 3] = [
    ("hand-drawn", "texture", Some("grain"), 2),
    ("scientific", "texture", Some("grid"), 1),
    ("sleek", "gradient", Some("glow"), 2),
];

#[test]
fn a_cascade_without_a_background_is_byte_for_byte_as_before() {
    for name in builtin_style_names() {
        let mut diagnostics = vec![];
        let plain = resolve_styles(&[(*name).to_owned()], &mut diagnostics);
        let ships = DEFAULTS.iter().any(|(preset, ..)| preset == name);
        let mut flat = vec![];
        for cleared in ["background: none\n", "background:\n  texture: none\n"] {
            let with = resolve_styles(&[(*name).to_owned(), cleared.to_owned()], &mut diagnostics);
            assert!(
                with.document
                    .pointer("/settings/background/derived")
                    .is_none(),
                "{name} / {cleared}"
            );
            if !ships {
                assert_eq!(plain.css, with.css, "{name} / {cleared}");
                assert_eq!(plain.custom_properties, with.custom_properties, "{name}");
            }
            flat.push(with);
        }
        // Both ways of saying "no background" give the same flat report.
        assert_eq!(flat[0].css, flat[1].css, "{name}");
        assert_eq!(
            flat[0].custom_properties, flat[1].custom_properties,
            "{name}"
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}

#[test]
fn three_presets_ship_a_background_and_every_ink_keeps_its_floor_over_it() {
    // Every main and extended preset as shipped, light and dark (Sleek, Midnight and Chalk are
    // dark), and the author's colours over them, light and dark.
    let mut cascades: Vec<(String, String)> = richview_core::preset_names()
        .iter()
        .map(|name| ((*name).to_owned(), format!("preset: {name}\n")))
        .collect();
    cascades.extend(bases().into_iter().filter(|(name, _)| name.contains('+')));
    let mut dark = 0;
    for (label, source) in &cascades {
        let (style, diagnostics) = resolve(source);
        assert!(
            diagnostics.iter().all(|d| d.level == "info"),
            "{label}: {diagnostics:?}"
        );
        let page = token(&style, "bg");
        if page.l < 0.5
            && style
                .document
                .pointer("/settings/background/derived")
                .is_some()
        {
            dark += 1;
        }
        if let Some(texture) = derived(&style, "texture") {
            floors_hold(&style, &[texture, page], label);
        }
        if let Some(stop) = derived(&style, "stop") {
            let ramp: Vec<Oklch> = (0..=32)
                .map(|i| color::mix(page, stop, f64::from(i) / 32.0))
                .collect();
            floors_hold(&style, &ramp, label);
        }
    }
    assert!(
        dark >= 3,
        "the floors were checked over dark backgrounds too ({dark})"
    );
    for name in builtin_style_names() {
        let (style, diagnostics) = resolve(&format!("preset: {name}\n"));
        assert!(
            diagnostics.iter().all(|d| d.level == "info"),
            "{name}: {diagnostics:?}"
        );
        let background = style.document.pointer("/settings/background");
        let Some((_, key, word, step)) = DEFAULTS.iter().find(|(preset, ..)| preset == name) else {
            assert!(
                background.and_then(|b| b.get("derived")).is_none(),
                "{name} stays flat: {background:?}"
            );
            continue;
        };
        let background = background.expect("a default background");
        assert_eq!(background[key].as_str(), *word, "{name}");
        // A preset that names no strength takes medium, step 2.
        assert_eq!(
            background["strength"].as_u64().unwrap_or(2),
            *step,
            "{name}"
        );
        let page = token(&style, "bg");
        // The texture's ink, or the gradient's stop, is the strongest pixel it draws.
        let strongest = derived(&style, if *key == "texture" { "texture" } else { "stop" })
            .expect("a drawn background");
        let ratio = color::contrast(strongest, page);
        assert!(
            ratio > 1.04,
            "{name}: the background is too faint to see ({ratio:.3}:1)"
        );
        let worst: Vec<Oklch> = if *key == "texture" {
            vec![strongest]
        } else {
            (0..=32)
                .map(|i| color::mix(page, strongest, f64::from(i) / 32.0))
                .collect()
        };
        floors_hold(&style, &worst, name);
        floors_hold(&style, &[page], name);
    }
}

/// The CSS a preset's default background adds: what differs between the preset as shipped and the
/// same preset with `background: none`, every token being equal.
fn generated_block(name: &str) -> String {
    let (with, _) = resolve(&format!("preset: {name}\n"));
    let (flat, _) = resolve(&format!("preset: {name}\nbackground: none\n"));
    assert_eq!(with.custom_properties, flat.custom_properties, "{name}");
    let (a, b) = (with.css.as_bytes(), flat.css.as_bytes());
    let head = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let tail = a[head..]
        .iter()
        .rev()
        .zip(b[head..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    with.css[head..with.css.len() - tail].to_owned()
}

/// Every `selector { declarations }` pair of a CSS text, at-rules flattened.
fn css_rules(css: &str) -> Vec<(String, String)> {
    let rule = regex_lite::Regex::new(r"([^{}]+)\{([^{}]*)\}").unwrap();
    rule.captures_iter(css)
        .map(|r| {
            let selector = r[1].trim();
            let selector = selector.rsplit('{').next().unwrap_or(selector).trim();
            (selector.to_owned(), r[2].trim().to_owned())
        })
        .collect()
}

#[test]
fn the_presets_backgrounds_keep_the_rules_the_presets_css_is_held_to() {
    // style_rules judges each preset's own CSS without this block; here the block is held to the
    // same rules: STY-37 (only floating elements cast a shadow, nothing gains one on hover),
    // STY-34 (gradients only where the background is painted, no side rules) and STY-61 (quotes,
    // code and total rows keep the preset's own rules: the block names none of them).
    let mut checked = 0;
    for name in richview_core::preset_names() {
        let block = generated_block(name);
        let ships = !block.is_empty();
        if !ships {
            continue;
        }
        checked += 1;
        let rules = css_rules(&block);
        assert!(!rules.is_empty(), "{name}: {block}");
        for (selector, body) in &rules {
            assert!(
                !selector.contains(":hover"),
                "{name}: `{selector}` reacts to hover"
            );
            for declaration in body.split(';') {
                let Some((property, value)) = declaration.split_once(':') else {
                    continue;
                };
                let (property, value) = (property.trim(), value.trim());
                if property == "box-shadow" {
                    // A pad of the surface's own colour around a solid block: no offset, no blur,
                    // nothing that reads as a cast shadow.
                    assert!(
                        value.starts_with("0 0 0 ") && value.ends_with("var(--rv-bg-chart)"),
                        "{name}: `{selector}` casts a shadow ({value})"
                    );
                }
                if value.contains("gradient(") || value.contains("url(") {
                    assert!(
                        ["html", ".rv-report__header::before", "body::before"]
                            .contains(&selector.as_str()),
                        "{name}: `{selector}` paints a gradient or picture"
                    );
                }
                assert!(
                    !matches!(
                        property,
                        "border-left" | "border-right" | "border-inline-start"
                    ),
                    "{name}: `{selector}` draws a side rule"
                );
            }
            for owned in [
                "pre",
                "code",
                "blockquote",
                "rv-callout",
                "tfoot",
                "rv-total",
                "th",
                "td",
            ] {
                assert!(
                    !selector
                        .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
                        .any(|part| part == owned),
                    "{name}: `{selector}` restyles {owned}, which the preset's rules own"
                );
            }
        }
    }
    assert!(
        checked >= 3,
        "Hand-drawn, Scientific and Sleek ship a background ({checked})"
    );
}

#[test]
fn a_presets_own_background_keeps_the_presets_colours_and_says_nothing() {
    for (name, ..) in DEFAULTS {
        let (style, diagnostics) = resolve(&format!("preset: {name}\n"));
        let (flat, _) = resolve(&format!("preset: {name}\nbackground: none\n"));
        assert_eq!(style.custom_properties, flat.custom_properties, "{name}");
        assert!(diagnostics.is_empty(), "{name}: {diagnostics:?}");
    }
}

#[test]
fn one_word_stands_for_a_texture_a_gradient_or_a_picture() {
    for (word, key) in [
        ("grain", "texture"),
        ("dots", "texture"),
        ("grid", "texture"),
        ("lines", "texture"),
        ("soft", "gradient"),
        ("glow", "gradient"),
        ("media/hero.png", "picture"),
        ("hero.JPG", "picture"),
        ("hero.webp", "picture"),
    ] {
        let (style, diagnostics) = resolve(&format!("preset: air\nbackground: {word}\n"));
        assert!(diagnostics.is_empty(), "{word}: {diagnostics:?}");
        assert_eq!(
            style
                .document
                .pointer(&format!("/settings/background/{key}"))
                .and_then(Value::as_str),
            Some(word),
            "{word}"
        );
    }
    // A word that is none of them says which words it takes.
    let (_, diagnostics) = resolve("preset: air\nbackground: tartan\n");
    let finding = diagnostics
        .iter()
        .find(|d| d.code == "style.schema")
        .expect("a finding");
    assert!(
        finding.message.contains("grain") && finding.message.contains("glow"),
        "{finding:?}"
    );
}

#[test]
fn a_later_file_replaces_a_background_whole_and_a_later_preset_clears_it() {
    let mut diagnostics = vec![];
    let replaced = resolve_styles(
        &[
            "air".into(),
            "background:\n  texture: grid\n".into(),
            "background:\n  gradient: soft\n".into(),
        ],
        &mut diagnostics,
    );
    let background = &replaced.document["settings"]["background"];
    assert!(background.get("texture").is_none(), "{background}");
    assert_eq!(background["gradient"], "soft");
    let cleared = resolve_styles(
        &["background:\n  texture: grid\n".into(), "air".into()],
        &mut diagnostics,
    );
    assert!(cleared.document.pointer("/settings/background").is_none());
    let none = resolve_styles(
        &[
            "air".into(),
            "background:\n  texture: grid\n".into(),
            "background: none\n".into(),
        ],
        &mut diagnostics,
    );
    assert!(
        !none.css.contains("linear-gradient"),
        "background: none clears it"
    );
}

#[test]
fn bad_background_leaves_are_findings_and_leave_the_rest() {
    for (file, code) in [
        ("background:\n  texure: grain\n", "style.unknown-key"),
        ("background:\n  texture: stripes\n", "style.schema"),
        (
            "background:\n  texture: grain\n  strength: huge\n",
            "style.schema",
        ),
        ("background:\n  place: sidebar\n", "style.schema"),
        ("background:\n  picture: ../outside.png\n", "style.schema"),
        (
            "background:\n  picture: https://example.invalid/a.png\n",
            "style.schema",
        ),
        ("background:\n  picture: /etc/a.png\n", "style.schema"),
        ("background: tartan\n", "style.schema"),
        (
            "background:\n  picture: hero.png\n  texture: grain\n",
            "style.background-conflict",
        ),
    ] {
        let (style, diagnostics) = resolve(&format!("preset: air\n{file}"));
        assert!(
            diagnostics.iter().any(|d| d.code == code),
            "{file}: expected {code}, got {diagnostics:?}"
        );
        // A bad strength leaves the texture at its default strength.
        if file.contains("huge") {
            assert!(derived(&style, "texture").is_some());
        }
    }
}

fn report(
    style: &str,
    data: &[(&str, String)],
    output: Output,
) -> richview_core::document::RenderedReport {
    let mut files: BTreeMap<String, String> = data
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect();
    files.insert("look.rstyle.yaml".into(), style.to_owned());
    let options = RenderOptions {
        report_folder: Some(richview_core::document::ReportFolder {
            root: "report".into(),
            source: "report.rmd".into(),
            files: std::iter::once("report.rmd".to_owned())
                .chain(files.keys().cloned())
                .collect(),
        }),
        data: files,
        output,
        ..RenderOptions::default()
    };
    let source = "---\ntitle: Orders doubled\nstyles: [look.rstyle.yaml]\n---\n\nThe launch week brought **1,240** orders.\n";
    render(source, &options).expect("report renders")
}

fn png(base64: &str) -> String {
    format!("data:image/png;base64,{base64}")
}

#[test]
fn a_picture_is_embedded_once_validated_and_left_off_paper() {
    let style = "preset: scientific\nbackground:\n  picture: media/hero.png\n";
    let screen = report(style, &[("media/hero.png", png(OK_PNG))], Output::Report);
    assert!(
        screen.diagnostics.iter().all(|d| d.level != "error"),
        "{:?}",
        screen.diagnostics
    );
    assert_eq!(screen.html.matches(OK_PNG).count(), 1, "embedded once");
    assert!(screen.html.contains(".rv-report__header::after"));
    let paper = report(style, &[("media/hero.png", png(OK_PNG))], Output::Pdf);
    assert!(!paper.html.contains(OK_PNG), "paper drops the picture");
    // A picture with an alpha channel is drawn over the page colour like any other.
    let alpha = report(
        "preset: air\nbackground:\n  picture: a.png\n  place: margins\n",
        &[("a.png", png(ALPHA_PNG))],
        Output::Report,
    );
    assert!(alpha.html.contains(ALPHA_PNG));
    assert!(alpha.diagnostics.iter().all(|d| d.level != "error"));
}

#[test]
fn a_missing_huge_wide_or_foreign_picture_is_an_error_and_renders_without_it() {
    let huge = format!(
        "data:image/png;base64,{}",
        "A".repeat((400_usize * 1024 + 3).div_ceil(3) * 4 + 8)
    );
    for (data, code) in [
        (vec![], "style.background-missing"),
        (vec![("hero.png", huge)], "style.background-size"),
        (
            vec![("hero.png", png(WIDE_PNG))],
            "style.background-dimensions",
        ),
        (
            vec![(
                "hero.png",
                "data:image/gif;base64,R0lGODlhAQABAAAAACw=".to_owned(),
            )],
            "style.background-format",
        ),
        (
            vec![("hero.png", "hostile text".to_owned())],
            "style.background-format",
        ),
    ] {
        let rendered = report(
            "preset: air\nbackground:\n  picture: hero.png\n",
            &data,
            Output::Report,
        );
        assert!(
            rendered.diagnostics.iter().any(|d| d.code == code
                && d.level == "error"
                && d.path.ends_with("background.picture")),
            "{code}: {:?}",
            rendered.diagnostics
        );
        assert!(!rendered.html.contains(".rv-report__header::after"));
    }
}

#[test]
fn textures_are_screen_only_and_never_reach_paper() {
    let style = "preset: air\nbackground:\n  texture: grid\n";
    let screen = report(style, &[], Output::Report);
    assert!(
        screen
            .html
            .contains("@media screen{html{background:linear-gradient")
    );
    let paper = report(style, &[], Output::Pdf);
    let printed = paper.html.split("@media screen").count();
    assert!(
        printed >= 2,
        "the texture CSS stays inside its screen query"
    );
    assert!(!paper.html.contains("@media print{html{background:linear"));
}

/// The text of every `<style>` element of a report.
fn css_of(html: &str) -> String {
    let mut css = String::new();
    let mut rest = html;
    while let Some(start) = rest.find("<style") {
        let end = rest[start..].find("</style>").expect("a closed style") + start;
        css.push_str(&rest[start..end]);
        rest = &rest[end..];
    }
    css
}

#[test]
fn a_picture_under_text_is_washed_over_the_whole_page_however_tall_it_renders() {
    let style = "preset: air\nbackground:\n  picture: hero.png\n  place: page\n";
    let rendered = report(style, &[("hero.png", png(OK_PNG))], Output::Report);
    let css = css_of(&rendered.html);
    let start = css
        .find("html{background:linear-gradient(to bottom,")
        .expect("the veil");
    let rule = &css[start..css[start..].find('}').unwrap() + start];
    // The veil is the first layer and covers the whole root box, so a picture taller than the
    // window is washed below the first screen too; the picture layer comes after it.
    let first = rule.find("no-repeat,").expect("layers");
    let (veil, picture) = (&rule[..first], &rule[first + "no-repeat,".len()..]);
    assert!(veil.ends_with("100vh) 0 0/100% 100% "), "{rule}");
    assert!(picture.starts_with("url(\"data:image/png"), "{rule}");
}

#[test]
fn table_blocks_keep_a_solid_surface_over_a_page_background() {
    let mut files = BTreeMap::new();
    files.insert(
        "look.rstyle.yaml".to_owned(),
        "preset: scientific\nbackground:\n  texture: grid\n".to_owned(),
    );
    let source = "---\ntitle: Orders\nstyles: [look.rstyle.yaml]\n---\n\n```table\ntitle: Orders by month\ndata:\n  inline:\n    - { month: Jan, orders: 120 }\n    - { month: Feb, orders: 240 }\n```\n";
    let options = RenderOptions {
        report_folder: Some(richview_core::document::ReportFolder {
            root: "report".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into(), "look.rstyle.yaml".into()],
        }),
        data: files,
        ..RenderOptions::default()
    };
    let rendered = render(source, &options).expect("report renders");
    assert!(
        rendered.html.contains("<div class=\"rv-table"),
        "a table block"
    );
    let css = css_of(&rendered.html);
    let solid = css
        .find(".rv-chart,.rv-table,")
        .expect("the solid surfaces");
    assert!(css[solid..].starts_with(
        ".rv-chart,.rv-table,.rv-table-scroll,.rv-embed{background:var(--rv-bg-chart)"
    ));
}

#[test]
fn hostile_background_files_last_over_every_preset_emit_only_generated_layers() {
    let hostile = [
        include_str!("hostile/s07-background-injection.rstyle.yaml"),
        include_str!("hostile/s08-background-picture-paths.rstyle.yaml"),
        include_str!("hostile/s08-background-picture-paths.second.rstyle.yaml"),
    ];
    for preset in builtin_style_names() {
        for (index, file) in hostile.iter().enumerate() {
            // Each hostile file over a valid background, resolved last.
            let valid = "background:\n  texture: grid\n  gradient: glow\n";
            let style = format!(
                "preset: {preset}\n{}",
                file.replace("preset: scientific\n", "")
            );
            for layered in [
                vec![style.clone()],
                vec![format!("preset: {preset}\n{valid}"), file.to_string()],
            ] {
                let mut files: BTreeMap<String, String> = BTreeMap::new();
                let mut refs = vec![];
                for (n, text) in layered.iter().enumerate() {
                    let name = format!("s{n}.rstyle.yaml");
                    files.insert(name.clone(), text.clone());
                    refs.push(name);
                }
                let source = format!(
                    "---\ntitle: Hostile\nstyles: [{}]\n---\n\nText.\n",
                    refs.join(", ")
                );
                let options = RenderOptions {
                    report_folder: Some(richview_core::document::ReportFolder {
                        root: "report".into(),
                        source: "report.rmd".into(),
                        files: std::iter::once("report.rmd".to_owned())
                            .chain(files.keys().cloned())
                            .collect(),
                    }),
                    data: files,
                    ..RenderOptions::default()
                };
                let rendered = render(&source, &options).expect("report renders");
                let label = format!("{preset} / file {index}");
                for marker in ["HOSTILE", "evil.example"] {
                    assert!(
                        !rendered.html.contains(marker),
                        "{label}: {marker} reached the page"
                    );
                }
                let css = css_of(&rendered.html);
                for marker in ["javascript:", "onload", "<script", "svg+xml,<"] {
                    assert!(!css.contains(marker), "{label}: {marker} reached the CSS");
                }
                let mut rest = css.as_str();
                while let Some(at) = rest.find("url(") {
                    let after = rest[at + 4..].trim_start_matches(['"', '\'']);
                    assert!(
                        after.starts_with("data:"),
                        "{label}: a CSS url() that is not generated: {}",
                        &after[..after.len().min(40)]
                    );
                    rest = &rest[at + 4..];
                }
            }
        }
    }
}

fn report_with(
    frontmatter: &str,
    data: &[(&str, String)],
) -> richview_core::document::RenderedReport {
    let files: BTreeMap<String, String> = data
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect();
    let options = RenderOptions {
        report_folder: Some(richview_core::document::ReportFolder {
            root: "report".into(),
            source: "report.rmd".into(),
            files: std::iter::once("report.rmd".to_owned())
                .chain(files.keys().cloned())
                .collect(),
        }),
        data: files,
        output: Output::Report,
        ..RenderOptions::default()
    };
    let source = format!(
        "---\ntitle: Orders doubled\n{frontmatter}---\n\nThe launch week brought **1,240** orders.\n"
    );
    render(&source, &options).expect("report renders")
}

#[test]
fn the_report_says_none_or_another_background_over_its_preset() {
    let preset = report_with("styles: [hand-drawn]\n", &[]);
    let flat = report_with("styles: [hand-drawn]\nbackground: none\n", &[]);
    let air = report_with("styles: [air]\n", &[]);
    let dots = report_with("styles: [hand-drawn]\nbackground: dots\n", &[]);
    let mapped = report_with(
        "styles: [hand-drawn]\nbackground:\n  gradient: soft\n  strength: small\n",
        &[],
    );
    for rendered in [&preset, &flat, &air, &dots, &mapped] {
        assert!(
            rendered.diagnostics.iter().all(|d| d.level == "info"),
            "{:?}",
            rendered.diagnostics
        );
    }
    let css = |r: &richview_core::document::RenderedReport| css_of(&r.html);
    assert_ne!(
        css(&preset),
        css(&flat),
        "hand-drawn ships grain; none takes it away"
    );
    assert!(
        css(&dots).contains("radial-gradient(circle,"),
        "dots replace the grain"
    );
    // Grain is the one texture drawn as an inline picture.
    const GRAIN: &str = "image/svg+xml;base64,";
    assert!(!css(&dots).contains(GRAIN));
    assert!(css(&preset).contains(GRAIN) && !css(&flat).contains(GRAIN));
    assert_ne!(css(&dots), css(&preset));
    assert!(
        css(&mapped).contains("linear-gradient(in oklch to bottom"),
        "the mapping applies"
    );
    // The frontmatter wins over a style file that names its own background.
    let file = report_with(
        "styles: [air, look.rstyle.yaml]\nbackground: none\n",
        &[("look.rstyle.yaml", "background: grid\n".to_owned())],
    );
    assert_eq!(css(&file), css(&air));
}

#[test]
fn a_bad_frontmatter_background_is_a_finding_at_the_frontmatter() {
    let rendered = report_with("styles: [air]\nbackground: tartan\n", &[]);
    let finding = rendered
        .diagnostics
        .iter()
        .find(|d| d.code == "style.schema")
        .unwrap_or_else(|| panic!("{:?}", rendered.diagnostics));
    assert_eq!(finding.path, "frontmatter.background");
    assert_eq!(finding.line, Some(4), "{finding:?}");
    assert!(finding.file.is_none());
}

#[test]
fn a_frontmatter_picture_is_bounded_like_a_style_files() {
    let huge = format!(
        "data:image/png;base64,{}",
        "A".repeat((400_usize * 1024 + 3).div_ceil(3) * 4 + 8)
    );
    for (picture, data, code) in [
        ("hero.png", vec![], "style.background-missing"),
        (
            "hero.png",
            vec![("hero.png", huge)],
            "style.background-size",
        ),
        ("../outside.png", vec![], "style.schema"),
        ("https://example.invalid/a.png", vec![], "style.schema"),
    ] {
        let rendered = report_with(&format!("styles: [air]\nbackground: {picture}\n"), &data);
        assert!(
            rendered.diagnostics.iter().any(|d| d.code == code),
            "{picture} / {code}: {:?}",
            rendered.diagnostics
        );
        assert!(!rendered.html.contains("data:image/png;base64,AAAA"));
    }
    let ok = report_with(
        "styles: [air]\nbackground: media/hero.png\n",
        &[("media/hero.png", png(OK_PNG))],
    );
    assert!(
        ok.diagnostics.iter().all(|d| d.level == "info"),
        "{:?}",
        ok.diagnostics
    );
    assert!(ok.html.contains(OK_PNG), "the picture is embedded");
}
