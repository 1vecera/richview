//! Measured style checks (STY-22, STY-25, STY-26, STY-27, STY-52).
//!
//! The same measures run over the five builtins in tests and over an author's cascade in
//! `check`: contrast floors for every ink, the type scale's steps, the weights a style
//! embeds, and the palette and diverging rules. Style-level findings are `style.*` warnings;
//! chart-level ones keep FEA-CH-60's and FEA-CH-62's `chart.*` codes.

use super::{ResolvedStyle, font_catalog};
use crate::Diagnostic;
use richview_chart::color::{self, Oklch};
use serde_json::Value;

/// Desktop steps are at least this much larger than the one below (STY-26).
pub const MIN_STEP: f64 = 1.2;
/// Phone steps (640 px and narrower) are at least this much larger.
pub const MIN_STEP_PHONE: f64 = 1.125;
/// A style embeds at most this many weights (STY-27).
pub const MAX_WEIGHTS: usize = 3;

/// Contrast floors on the page, raised and chart surfaces (STY-25), with the tokens that carry them.
pub const FLOORS: [(&str, f64); 10] = [
    ("ink", 7.0),
    ("chart-ink", 7.0),
    ("ink-soft", 4.5),
    ("link", 4.5),
    ("brand", 4.5),
    ("up", 4.5),
    ("down", 4.5),
    ("hold", 4.5),
    ("accent", 3.0),
    ("ink-faint", 3.0),
];

const SURFACES: [&str; 3] = ["bg", "bg-elev", "bg-chart"];
const STEPS: [&str; 6] = ["small", "base", "h3", "h2", "h1", "display"];

fn token(style: &ResolvedStyle, name: &str) -> Option<String> {
    style.document["tokens"][name].as_str().map(str::to_owned)
}

fn colour(style: &ResolvedStyle, name: &str) -> Option<Oklch> {
    token(style, name).and_then(|css| color::parse(&css))
}

fn warning(code: &str, path: &str, message: String) -> Diagnostic {
    Diagnostic::new("warning", code, message, path)
}

/// Tokens below their contrast floor on any of the three surfaces.
pub fn contrast(style: &ResolvedStyle) -> Vec<Diagnostic> {
    let mut found = vec![];
    let surfaces: Vec<(&str, Oklch)> = SURFACES
        .iter()
        .filter_map(|name| colour(style, name).map(|c| (*name, c)))
        .collect();
    for (name, floor) in FLOORS {
        let Some(ink) = colour(style, name) else {
            continue;
        };
        for (surface, background) in &surfaces {
            let ratio = color::contrast(ink, *background);
            if ratio < floor - 1e-9 {
                found.push(warning(
                    "style.contrast",
                    &format!("tokens.{name}"),
                    format!("'{name}' reaches {ratio:.2}:1 on '{surface}'; its floor is {floor}:1"),
                ));
                break;
            }
        }
    }
    found
}

fn pixels(value: &str) -> Option<f64> {
    value.strip_suffix("px").unwrap_or(value).parse().ok()
}

/// Type steps closer than the minimum ratio, on desktop and on phones.
pub fn type_scale(style: &ResolvedStyle) -> Vec<Diagnostic> {
    let mut found = vec![];
    for (suffix, minimum, label) in [
        ("", MIN_STEP, "desktop"),
        ("-phone", MIN_STEP_PHONE, "phone"),
    ] {
        let sizes: Vec<Option<f64>> = STEPS
            .iter()
            .map(|step| {
                token(style, &format!("size-{step}{suffix}"))
                    .or_else(|| token(style, &format!("size-{step}")))
                    .and_then(|v| pixels(&v))
            })
            .collect();
        for pair in 1..STEPS.len() {
            let (Some(low), Some(high)) = (sizes[pair - 1], sizes[pair]) else {
                continue;
            };
            if high / low < minimum - 1e-9 {
                found.push(warning(
                    "style.type-scale",
                    &format!("tokens.size-{}{suffix}", STEPS[pair]),
                    format!(
                        "On {label}, '{}' is {:.3} times '{}'; each step must be at least {minimum} times the one below",
                        STEPS[pair],
                        high / low,
                        STEPS[pair - 1]
                    ),
                ));
            }
        }
    }
    found
}

/// More than [`MAX_WEIGHTS`] distinct weights among the faces the style embeds.
pub fn weights(style: &ResolvedStyle) -> Vec<Diagnostic> {
    let mut weights: Vec<u32> = style
        .font_css
        .split("font-weight:")
        .skip(1)
        .filter_map(|rest| rest.trim_start().split(';').next()?.trim().parse().ok())
        .collect();
    weights.sort_unstable();
    weights.dedup();
    if weights.len() > MAX_WEIGHTS {
        return vec![warning(
            "style.weights",
            "tokens.font-text",
            format!(
                "The style embeds {} weights ({}); at most {MAX_WEIGHTS} keep a report light",
                weights.len(),
                weights
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )];
    }
    vec![]
}

/// A declared face the catalogue does not embed: the first family of each font role must be
/// bundled, a generic family, or a brand font the style declares (TYP-46, STY-16).
pub fn fonts(style: &ResolvedStyle) -> Vec<Diagnostic> {
    const GENERIC: [&str; 7] = [
        "serif",
        "sans-serif",
        "monospace",
        "system-ui",
        "ui-monospace",
        "ui-serif",
        "ui-sans-serif",
    ];
    let catalog = font_catalog();
    if catalog.is_empty() {
        return vec![];
    }
    // A brand font counts once its file is embedded; a role whose file is absent has already
    // been reported by the embedding.
    let declared: Vec<String> = style
        .font_css
        .split("font-family: \"")
        .skip(1)
        .filter_map(|tail| tail.split('"').next())
        .map(str::to_lowercase)
        .collect();
    let branded: Vec<&str> = style.document["settings"]["brand-fonts"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(role, _)| role.as_str())
        .collect();
    let mut found = vec![];
    for role in ["display", "text", "ui", "mono"] {
        if branded.contains(&role) {
            continue;
        }
        let Some(stack) = token(style, &format!("font-{role}")) else {
            continue;
        };
        let family = super::first_family(&stack);
        let known = GENERIC.contains(&family.as_str())
            || declared.contains(&family)
            || catalog.iter().any(|(name, _)| *name == family);
        if !known {
            found.push(warning(
                "style.font-missing",
                &format!("tokens.font-{role}"),
                format!(
                    "Family '{family}' has no bundled face and no declared file; the report falls back to the next family of its stack"
                ),
            ));
        }
    }
    found
}

/// Code points each bundled family embeds, one `<family>\t<ranges>` line per family; generated
/// from the font manifest by `scripts/fonts/build.py`.
const COVERAGE: &str = include_str!("../../assets/font-coverage.txt");

/// A stretch of report text to scan: where it starts, which font role sets it (`text` or
/// `display`) and whether it is Markdown, whose code is set in the mono face and skipped.
pub struct GlyphScan<'a> {
    pub text: &'a str,
    pub first_line: usize,
    pub role: &'a str,
    pub markdown: bool,
}

/// The code points the faces of one role's font stack embed.
fn role_coverage(style: &ResolvedStyle, role: &str) -> Vec<(u32, u32)> {
    let embedded: Vec<String> = style
        .font_css
        .split("font-family: \"")
        .skip(1)
        .filter_map(|tail| tail.split('"').next())
        .map(str::to_lowercase)
        .collect();
    let stack = token(style, &format!("font-{role}")).unwrap_or_default();
    // A character the first family lacks falls to the next embedded family of the stack.
    let families: Vec<String> = stack
        .split(',')
        .map(|f| f.trim().trim_matches(['\'', '"']).to_lowercase())
        .filter(|f| embedded.contains(f))
        .collect();
    // A brand font's coverage is its file's own, which the check cannot read: a stack that
    // starts with one is taken to cover its text rather than warn on every character.
    let brand = style
        .font_css
        .lines()
        .filter(|line| line.starts_with("/* brand font */"))
        .filter_map(|line| line.split("font-family: \"").nth(1)?.split('"').next())
        .map(str::to_lowercase)
        .any(|family| families.contains(&family));
    if brand {
        return vec![(0, char::MAX as u32)];
    }
    let mut covered = vec![];
    for row in COVERAGE.lines() {
        let Some((family, ranges)) = row.split_once('\t') else {
            continue;
        };
        if families.iter().any(|name| name == family) {
            for part in ranges.split(',') {
                let (low, high) = part.split_once('-').unwrap_or((part, part));
                if let (Ok(low), Ok(high)) =
                    (u32::from_str_radix(low, 16), u32::from_str_radix(high, 16))
                {
                    covered.push((low, high));
                }
            }
        }
    }
    covered
}

/// Characters of the scanned text outside the faces its role's stack embeds, as one
/// `style.glyph-missing` warning for the report on the first line of the first. Fenced and
/// inline code in Markdown, whitespace, control characters, joiners and variation selectors are
/// skipped; a generic fallback stack does not cover a character, because the check is about the
/// embedded faces.
pub fn glyphs(style: &ResolvedStyle, scans: &[GlyphScan]) -> Vec<Diagnostic> {
    if !style.font_css.contains("font-family: \"") {
        return vec![];
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut missing: Vec<(char, usize)> = vec![];
    for scan in scans {
        let covered = role_coverage(style, scan.role);
        let mut fence: Option<(char, usize)> = None;
        for (index, line) in scan.text.lines().enumerate() {
            if scan.markdown {
                let trimmed = line.trim_start();
                let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
                if let Some(mark) = marker {
                    let run = trimmed.chars().take_while(|c| *c == mark).count();
                    match fence {
                        Some((open, length)) if open == mark && run >= length => {
                            fence = None;
                            continue;
                        }
                        None if run >= 3 => {
                            fence = Some((mark, run));
                            continue;
                        }
                        _ => {}
                    }
                }
                if fence.is_some() {
                    continue;
                }
            }
            let mut in_code = false;
            for c in line.chars() {
                if scan.markdown && c == '`' {
                    in_code = !in_code;
                    continue;
                }
                let point = c as u32;
                if in_code
                    || c.is_whitespace()
                    || c.is_control()
                    || point == 0x200D
                    || (0xFE00..=0xFE0F).contains(&point)
                    || (0x200B..=0x200F).contains(&point)
                    || covered
                        .iter()
                        .any(|(low, high)| (*low..=*high).contains(&point))
                    || !seen.insert(c)
                {
                    continue;
                }
                missing.push((c, scan.first_line + index));
            }
        }
    }
    let Some(&(_, line)) = missing.first() else {
        return vec![];
    };
    let examples = missing
        .iter()
        .take(5)
        .map(|(c, _)| format!("'{c}' (U+{:04X})", *c as u32))
        .collect::<Vec<_>>()
        .join(", ");
    let mut finding = warning(
        "style.glyph-missing",
        "styles",
        format!(
            "{} character(s) are outside the faces the style embeds, such as {examples}; readers see them in a fallback font",
            missing.len()
        ),
    );
    finding.line = Some(line);
    vec![finding]
}

/// The palette rule over the style's categorical palette, on its chart and page surfaces.
pub fn palette(style: &ResolvedStyle) -> Vec<String> {
    let colors: Vec<String> = match &style.chart["palette"] {
        Value::String(name) => richview_chart::rmd_render::palette(name)
            .iter()
            .map(|c| (*c).to_owned())
            .collect(),
        Value::Array(list) => list
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => return vec![],
    };
    let surfaces: Vec<(&str, Oklch)> = ["bg-chart", "bg"]
        .into_iter()
        .filter_map(|name| colour(style, name).map(|c| (name, c)))
        .collect();
    let palette: Vec<&str> = colors.iter().map(String::as_str).collect();
    color::palette_issues(&palette, &surfaces)
}

/// A heatmap ramp and, when the style gives one, its negative arm.
pub fn ramps(style: &ResolvedStyle) -> Vec<String> {
    let strings = |key: &str| -> Vec<String> {
        style.chart[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let surfaces: Vec<(&str, Oklch)> = ["bg-chart", "bg"]
        .into_iter()
        .filter_map(|name| colour(style, name).map(|c| (name, c)))
        .collect();
    let mut found = vec![];
    for (key, label) in [("ramp", "ramp"), ("negative-ramp", "negative ramp")] {
        let stops = strings(key);
        if stops.is_empty() {
            continue;
        }
        let refs: Vec<&str> = stops.iter().map(String::as_str).collect();
        found.extend(
            color::ramp_issues(&refs, &surfaces)
                .into_iter()
                .map(|issue| format!("{label}: {issue}")),
        );
    }
    let (positive, negative) = (strings("ramp"), strings("negative-ramp"));
    if !positive.is_empty() && !negative.is_empty() {
        let (p, n): (Vec<&str>, Vec<&str>) = (
            positive.iter().map(String::as_str).collect(),
            negative.iter().map(String::as_str).collect(),
        );
        // A style that marks the sign another way needs no colour difference (STY-56).
        if style.chart["sign-cue"].as_str() != Some("hatch") {
            found.extend(color::diverging_issues(&p, &n));
        }
    }
    found
}

/// Every style-level finding of a resolved cascade, as `style.*` warnings. The builtins have
/// none (STY-52); `check` runs this over an author's cascade.
pub fn style_findings(style: &ResolvedStyle) -> Vec<Diagnostic> {
    let mut found = contrast(style);
    found.extend(type_scale(style));
    found.extend(weights(style));
    found.extend(fonts(style));
    found
}
