//! The closed vocabulary of a style file (TYP-44..TYP-50, STY-14, STY-15, STY-67).
//!
//! The catalogue validates author settings and the internal definitions of builtins. Retired
//! author leaves produce a located warning and never enter the cascade. The supported settings
//! expand into the sections shared with presets.

use super::{
    PALETTE_NAMES, builtin_style_names, chart_color, color_list, valid_color, valid_dim,
    valid_font, valid_letter_spacing, valid_shadow,
};
use crate::Diagnostic;
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, sync::OnceLock};

/// Whether an agent may play with a leaf freely (`Safe`), may break the layout with it
/// (`Experimental`), may not set it at all (`Internal`: it fixes the page's mechanics), or sets
/// a retired author key that a builtin may still use internally (`Legacy`, STY-73).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Mark {
    Safe,
    Experimental,
    Internal,
    Legacy,
}

/// The values a leaf accepts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Grammar {
    /// A CSS colour in a closed grammar (TYP-46).
    Colour,
    /// A font stack of safe characters.
    Font,
    /// A non-negative number or a short length.
    Dimension,
    /// `none` or `x y blur` and one colour of the closed grammar.
    Shadow,
    /// A number in a range.
    Number(f64, f64),
    Boolean,
    /// One of these words.
    Enum(&'static [&'static str]),
    /// A font weight word or a number from 1 to 1000.
    Weight,
    /// `normal` or a signed length.
    LetterSpacing,
    /// A builtin palette name, or 1 to 8 colours.
    Palette,
    /// Exactly this many colours.
    Colours(usize),
    /// A bundled family stack, or `{family, file}` for a brand font in the report folder (STY-16).
    FontSetting,
    /// A builtin style name.
    Builtin,
    /// An opaque colour the chart engine can interpret.
    BrandColour,
    /// A company name, rendered as plain text.
    BrandName,
    /// A report-relative raster image path.
    BrandLogo,
    /// One of the five steps, `xsmall` to `xlarge` (STY-73).
    Step,
    /// A builtin or extended preset name.
    Preset,
}

use serde::Serialize;

/// One leaf of the vocabulary.
#[derive(Debug, Clone, Serialize)]
pub struct Leaf {
    pub path: String,
    #[serde(skip)]
    pub grammar: Grammar,
    pub mark: Mark,
    pub about: &'static str,
}

const SECTIONS: [&str; 5] = ["tokens", "layout", "chart", "motion", "components"];
const COLOUR_TOKENS: [(&str, &str); 16] = [
    ("bg", "Page surface"),
    ("bg-elev", "Raised surface: code, readout"),
    ("bg-chart", "Chart surface"),
    ("ink", "Text"),
    ("ink-soft", "Secondary text"),
    ("ink-faint", "Ticks and leaders; never text"),
    ("chart-ink", "Axis text and the baseline"),
    ("rule", "Rules and hairlines"),
    ("rule-soft", "Gridlines"),
    ("accent", "The one accent: marks, rules, focus rings"),
    ("accent-soft", "Band and brush fills"),
    ("link", "Links and the table's row summary"),
    (
        "brand",
        "The style's shade of the brand blue, for the wordmark",
    ),
    ("up", "Rising delta and callout title"),
    ("down", "Falling delta and callout title"),
    ("hold", "Hold callout title"),
];
const SIZE_STEPS: [&str; 6] = ["small", "base", "h3", "h2", "h1", "display"];
const TEXT_PROPERTIES: [&str; 4] = ["title", "heading", "table-head", "source-label"];
const FONT_ROLES: [&str; 4] = ["display", "text", "ui", "mono"];

fn build() -> Vec<Leaf> {
    let mut leaves = vec![];
    let mut add = |path: &str, grammar, mark, about| {
        leaves.push(Leaf {
            path: path.to_owned(),
            grammar,
            mark,
            about,
        });
    };
    for (name, about) in COLOUR_TOKENS {
        add(
            &format!("tokens.{name}"),
            Grammar::Colour,
            Mark::Legacy,
            about,
        );
    }
    for role in FONT_ROLES {
        add(
            &format!("tokens.font-{role}"),
            Grammar::Font,
            Mark::Safe,
            "A font stack; the first family must be bundled or declared",
        );
    }
    for step in SIZE_STEPS {
        for suffix in ["", "-phone"] {
            add(
                &format!("tokens.size-{step}{suffix}"),
                Grammar::Dimension,
                Mark::Experimental,
                "One step of the type scale; `type-size` scales the whole scale",
            );
        }
    }
    add(
        "tokens.line-base",
        Grammar::Dimension,
        Mark::Experimental,
        "Line height of body text",
    );
    add(
        "tokens.grid",
        Grammar::Colour,
        Mark::Internal,
        "The chart's soft rules: gridlines, tracks and empty cells (the `gridlines` setting)",
    );
    add(
        "tokens.on-accent",
        Grammar::Colour,
        Mark::Internal,
        "Text set on the accent, as in a knockout metric: white or black, whichever stands out more",
    );
    add(
        "tokens.measure",
        Grammar::Dimension,
        Mark::Internal,
        "The reading measure",
    );
    add(
        "tokens.print-measure",
        Grammar::Dimension,
        Mark::Internal,
        "The builtin reading measure on paper, before the print type scale",
    );
    add(
        "tokens.radius-s",
        Grammar::Dimension,
        Mark::Safe,
        "Small corners: inline code, the readout",
    );
    add(
        "tokens.radius-m",
        Grammar::Dimension,
        Mark::Safe,
        "Medium corners: images, embeds, code",
    );
    add(
        "tokens.shadow-float",
        Grammar::Shadow,
        Mark::Safe,
        "The shadow of floating elements",
    );
    add(
        "tokens.scheme",
        Grammar::Enum(&["light", "dark"]),
        Mark::Safe,
        "The colour scheme the page colours make",
    );
    add(
        "tokens.print-as",
        Grammar::Builtin,
        Mark::Safe,
        "The light builtin a dark style prints as",
    );
    add(
        "tokens.density",
        Grammar::Number(0.5, 2.0),
        Mark::Safe,
        "Space inside blocks, as a multiplier",
    );
    add(
        "tokens.block-gap",
        Grammar::Dimension,
        Mark::Safe,
        "Space between blocks and before sections",
    );
    add(
        "tokens.block-gap-phone",
        Grammar::Dimension,
        Mark::Safe,
        "The block gap on phones",
    );
    add(
        "layout.report-max-width",
        Grammar::Dimension,
        Mark::Internal,
        "The report column's width",
    );

    add(
        "motion.enabled",
        Grammar::Boolean,
        Mark::Safe,
        "`false` renders charts settled",
    );
    add(
        "motion.character",
        Grammar::Enum(&["still", "restrained", "gentle", "playful", "crisp"]),
        Mark::Safe,
        "The style's motion in one word; the blocks, charts and controls follow it",
    );
    add(
        "motion.duration",
        Grammar::Number(0.0, 2000.0),
        Mark::Safe,
        "How long a block or a chart's data takes to arrive, in ms",
    );
    add(
        "motion.easing",
        Grammar::Enum(&["standard", "out", "soft", "spring", "snap"]),
        Mark::Safe,
        "The curve of that arrival",
    );
    add(
        "motion.stagger",
        Grammar::Number(0.0, 120.0),
        Mark::Safe,
        "The gap between marks that arrive one after another, in ms",
    );
    add(
        "motion.block-entrance",
        Grammar::Enum(&["none", "fade", "fade-up"]),
        Mark::Safe,
        "How blocks below the first screen arrive (FEA-RP-35)",
    );
    add(
        "motion.distance",
        Grammar::Number(0.0, 40.0),
        Mark::Safe,
        "How far a fade-up block rises, in px",
    );

    add(
        "components.metric.style",
        Grammar::Enum(&["rule", "bracket", "knockout", "underline"]),
        Mark::Experimental,
        "How a metric is framed",
    );
    add(
        "components.deck.style",
        Grammar::Enum(&["italic", "normal"]),
        Mark::Safe,
        "The line under the title",
    );
    add(
        "components.thumbnail.rule",
        Grammar::Enum(&["none", "hairline", "ink-bars", "pencil"]),
        Mark::Experimental,
        "The structural mark of the story's link-preview thumbnail (STY-70)",
    );
    add(
        "components.callout.style",
        Grammar::Enum(&["rule", "bracket"]),
        Mark::Experimental,
        "A callout under a rule, or beside a bracket",
    );
    add(
        "components.callout.border-width",
        Grammar::Dimension,
        Mark::Safe,
        "The callout rule's weight",
    );
    add(
        "components.header.border-top",
        Grammar::Dimension,
        Mark::Safe,
        "The bar above the header",
    );
    add(
        "components.header.border-bottom",
        Grammar::Dimension,
        Mark::Safe,
        "The bar below the header",
    );
    add(
        "components.rules.style",
        Grammar::Enum(&["hairline", "pencil"]),
        Mark::Safe,
        "Hairline or pencil rules",
    );
    for name in TEXT_PROPERTIES {
        add(
            &format!("components.{name}.transform"),
            Grammar::Enum(&["none", "uppercase"]),
            Mark::Safe,
            "Letter case",
        );
        add(
            &format!("components.{name}.weight"),
            Grammar::Weight,
            Mark::Safe,
            "Font weight",
        );
        add(
            &format!("components.{name}.letter-spacing"),
            Grammar::LetterSpacing,
            Mark::Safe,
            "Tracking",
        );
        add(
            &format!("components.{name}.font"),
            Grammar::Enum(&FONT_ROLES),
            Mark::Safe,
            "Which face role it uses",
        );
    }

    add(
        "components.figure.frame",
        Grammar::Enum(&["none", "shaded", "outline", "rule-above"]),
        Mark::Safe,
        "How a chart's figure is framed: nothing, a shaded panel, an outline or a rule above",
    );
    add(
        "components.figure.title",
        Grammar::Enum(&["above", "below"]),
        Mark::Safe,
        "Where the chart title sits relative to the plot",
    );
    add(
        "components.figure.description",
        Grammar::Enum(&["above", "below", "below-boxed"]),
        Mark::Safe,
        "Where the chart description sits: above the plot, below it, or below it in a box",
    );
    add(
        "components.slides.stage",
        Grammar::Enum(&[
            "plain",
            "keynote",
            "gallery",
            "poster",
            "sketchbook",
            "paper",
        ]),
        Mark::Safe,
        "How a slides source stands on its stage: the layout alone, or the world of a builtin style",
    );
    add(
        "chart.palette",
        Grammar::Palette,
        Mark::Safe,
        "Series colours",
    );
    add(
        "chart.ramp",
        Grammar::Colours(3),
        Mark::Safe,
        "Sequential ramp, low to high",
    );
    add(
        "chart.negative-ramp",
        Grammar::Colours(3),
        Mark::Safe,
        "Ramp for negative heatmap values",
    );
    add(
        "chart.font-family",
        Grammar::Font,
        Mark::Safe,
        "Face of all chart lettering",
    );
    add(
        "chart.font-numeric",
        Grammar::Font,
        Mark::Safe,
        "Tabular-figure face of ticks and readout values",
    );
    add(
        "chart.bar-radius",
        Grammar::Number(0.0, 64.0),
        Mark::Safe,
        "Rounding of bar ends",
    );
    add(
        "chart.decals",
        Grammar::Boolean,
        Mark::Safe,
        "Hatches and dashes that stand in for colour",
    );
    add(
        "chart.glow",
        Grammar::Enum(&["medium", "flat"]),
        Mark::Internal,
        "How a highlighted mark glows: the soft glow, or one flat band in a pen-drawn style",
    );
    add(
        "chart.curve",
        Grammar::Enum(&["linear", "monotone", "step", "step_before", "step_after"]),
        Mark::Safe,
        "Default line curve",
    );
    add(
        "chart.curvature",
        Grammar::Number(0.0, 1.0),
        Mark::Safe,
        "How strongly a curve bends",
    );
    add(
        "chart.sign-cue",
        Grammar::Enum(&["none", "hatch"]),
        Mark::Safe,
        "How negative heatmap cells show their sign beyond colour",
    );
    add(
        "chart.legend",
        Grammar::Enum(&["top", "bottom"]),
        Mark::Safe,
        "Where the legend sits",
    );
    add(
        "chart.line.smoothing",
        Grammar::Enum(&["straight", "monotone", "current"]),
        Mark::Safe,
        "Line smoothing (replaced by curve)",
    );

    // The settings of STY-15 and the colophon of STY-04.
    add(
        "density",
        Grammar::Enum(&["comfortable", "loose", "airmax"]),
        Mark::Safe,
        "Space inside blocks: x1, x1.25, x1.5",
    );
    add(
        "block-gap",
        Grammar::Enum(&["regular", "tight", "wide"]),
        Mark::Safe,
        "Space between blocks: 4, 3 or 5 rem",
    );
    add(
        "type-size",
        Grammar::Enum(&["large", "small", "bigger"]),
        Mark::Safe,
        "The whole type scale x1, x0.9, x1.1",
    );
    add(
        "accent",
        Grammar::Colour,
        Mark::Safe,
        "The accent, first series and links",
    );
    add("palette", Grammar::Palette, Mark::Safe, "Series colours");
    add(
        "company.color",
        Grammar::BrandColour,
        Mark::Safe,
        "The company's primary colour",
    );
    add(
        "company.secondary",
        Grammar::BrandColour,
        Mark::Safe,
        "An optional second brand colour",
    );
    add(
        "company.base",
        Grammar::Builtin,
        Mark::Safe,
        "The builtin style underneath the brand",
    );
    add(
        "company.name",
        Grammar::BrandName,
        Mark::Safe,
        "The company name",
    );
    add(
        "company.logo",
        Grammar::BrandLogo,
        Mark::Safe,
        "A report-relative raster logo",
    );
    add(
        "company.logo-dark",
        Grammar::BrandLogo,
        Mark::Safe,
        "An optional logo for dark styles",
    );
    add(
        "motion",
        Grammar::Step,
        Mark::Safe,
        "How far and how long blocks and charts move: `xsmall` is still",
    );
    add(
        "colophon",
        Grammar::Enum(&["show", "hide"]),
        Mark::Safe,
        "The 'Made with RichView' line",
    );
    for role in FONT_ROLES {
        add(
            &format!("fonts.{role}"),
            Grammar::FontSetting,
            Mark::Safe,
            "A bundled family or a brand font file",
        );
    }

    // STY-73: a preset, the author's colours and the five-step settings.
    add(
        "preset",
        Grammar::Preset,
        Mark::Safe,
        "The preset the file starts from",
    );
    add(
        "colors.brand",
        Grammar::BrandColour,
        Mark::Safe,
        "Your colour: the accent, links and the first series; RichView derives the rest",
    );
    add(
        "colors.secondary",
        Grammar::BrandColour,
        Mark::Safe,
        "An optional second colour: the second series",
    );
    add(
        "colors.background",
        Grammar::BrandColour,
        Mark::Safe,
        "An optional page colour; a dark one makes a dark report",
    );
    for setting in super::settings::SETTINGS {
        if setting.name != "motion" {
            add(setting.name, Grammar::Step, Mark::Safe, setting.about);
        }
    }

    // STY-74: the background behind the report, in closed words and a picture path.
    add(
        "background",
        Grammar::Enum(&super::background::WORDS),
        Mark::Safe,
        "The background behind the report: one word (a texture, a gradient or a picture file), a mapping, or `none` for a flat page against a preset's or an earlier file's",
    );
    add(
        "background.texture",
        Grammar::Enum(&super::background::TEXTURES),
        Mark::Safe,
        "A texture drawn in a colour derived from the page: grain (paper), dots, grid (graph paper) or lines (ruled on the text's line pitch)",
    );
    add(
        "background.gradient",
        Grammar::Enum(&super::background::GRADIENTS),
        Mark::Safe,
        "A gradient from your colours: soft (a wash from the top) or glow (from the top corner); without colors, the preset's accent",
    );
    add(
        "background.picture",
        Grammar::BrandLogo,
        Mark::Safe,
        "Your picture: a still PNG, JPEG or WebP in the report folder, at most 400 KiB and 2400 px; it stands alone",
    );
    add(
        "background.place",
        Grammar::Enum(&super::background::PLACES),
        Mark::Safe,
        "Where it sits: page (behind everything; text keeps its contrast), margins (around the report, which sits on a solid sheet) or header (behind the title; a picture puts the title on a solid panel). Default page, header for a picture",
    );
    add(
        "background.strength",
        Grammar::Step,
        Mark::Safe,
        "How visible it is: xsmall to xlarge, medium the designed default; never past what text needs",
    );

    // Presets own their internal leaves. Author files receive migration advice (STY-73).
    for leaf in &mut leaves {
        let section = SECTIONS
            .iter()
            .any(|section| leaf.path.starts_with(&format!("{section}.")));
        let old_setting = matches!(
            leaf.path.as_str(),
            "density"
                | "block-gap"
                | "type-size"
                | "accent"
                | "company.color"
                | "company.secondary"
                | "company.base"
        );
        if (section && leaf.mark != Mark::Internal) || old_setting {
            leaf.mark = Mark::Legacy;
        }
    }
    leaves
}

/// The supported setting nearest to a retired leaf, when one exists.
fn retired_replacement(path: &str) -> Option<&'static str> {
    let leaf = path
        .split_once('.')
        .map_or(path, |(_, rest)| rest)
        .trim_end_matches("-phone");
    Some(match path {
        "density" | "tokens.density" => "padding",
        "block-gap" => "spacing",
        "type-size" => "text-size",
        "accent" | "company.color" => "colors.brand",
        "company.secondary" => "colors.secondary",
        "company.base" => "preset",
        "chart.bar-radius" => "corners",
        "chart.palette" => "palette",
        "chart.ramp" | "chart.negative-ramp" => "colors.brand",
        "components.callout.border-width"
        | "components.header.border-top"
        | "components.header.border-bottom" => "lines",
        _ if path.starts_with("motion.") => "motion",
        _ if path.starts_with("tokens.") => match leaf {
            "bg" | "bg-elev" | "bg-chart" | "scheme" => "colors.background",
            "accent" | "accent-soft" | "link" => "colors.brand",
            "ink" | "ink-soft" | "ink-faint" | "chart-ink" | "rule" => "tint",
            "rule-soft" => "gridlines",
            "radius-s" | "radius-m" => "corners",
            "block-gap" => "spacing",
            _ if leaf.starts_with("size-") => "text-size",
            "font-display" => "fonts.display",
            "font-text" => "fonts.text",
            "font-ui" => "fonts.ui",
            "font-mono" => "fonts.mono",
            _ => return None,
        },
        _ => return None,
    })
}

/// Every leaf of the style vocabulary, in a stable order. `richview learn`, the docs and
/// editor completions read it.
pub fn leaves() -> &'static [Leaf] {
    static LEAVES: OnceLock<Vec<Leaf>> = OnceLock::new();
    LEAVES.get_or_init(build)
}

fn index() -> &'static BTreeMap<&'static str, &'static Leaf> {
    static INDEX: OnceLock<BTreeMap<&'static str, &'static Leaf>> = OnceLock::new();
    INDEX.get_or_init(|| {
        leaves()
            .iter()
            .map(|leaf| (leaf.path.as_str(), leaf))
            .collect()
    })
}

/// Whether `path` is a mapping some leaf lives under (`components.title`, `chart.line`).
fn is_branch(path: &str) -> bool {
    let prefix = format!("{path}.");
    SECTIONS.contains(&path)
        || path == "fonts"
        || leaves().iter().any(|leaf| leaf.path.starts_with(&prefix))
}

/// Retired leaves still say why they are gone; they are skipped like any bad leaf.
fn retired(path: &str) -> Option<&'static str> {
    crate::registry::RETIRED_STYLE_FIELDS
        .iter()
        .find(|(old, _)| *old == path)
        .map(|(_, reason)| *reason)
}

fn accepts(grammar: Grammar, value: &Value) -> bool {
    match grammar {
        Grammar::Colour => value.as_str().is_some_and(valid_color),
        Grammar::Font => value.as_str().is_some_and(valid_font),
        Grammar::Dimension => valid_dim(value),
        Grammar::Shadow => value.as_str().is_some_and(valid_shadow),
        // Properties arrive as text, so a numeric string counts.
        Grammar::Number(low, high) => value
            .as_f64()
            .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
            .is_some_and(|n: f64| (low..=high).contains(&n)),
        Grammar::Boolean => value.is_boolean(),
        Grammar::Enum(words) => value.as_str().is_some_and(|word| words.contains(&word)),
        Grammar::Weight => {
            matches!(
                value.as_str(),
                Some("normal" | "bold" | "lighter" | "bolder")
            ) || crate::text(value)
                .parse::<u32>()
                .is_ok_and(|n| (1..=1000).contains(&n))
        }
        Grammar::LetterSpacing => valid_letter_spacing(value),
        Grammar::Palette => {
            value
                .as_str()
                .is_some_and(|name| PALETTE_NAMES.contains(&name))
                || color_list(value, 1..=8)
        }
        Grammar::Colours(n) => color_list(value, n..=n),
        Grammar::FontSetting => match value {
            Value::String(stack) => valid_font(stack),
            Value::Object(map) => {
                map.get("family")
                    .and_then(Value::as_str)
                    .is_some_and(valid_font)
                    && map
                        .get("file")
                        .and_then(Value::as_str)
                        .is_some_and(valid_font_file)
                    && map.get("weight").is_none_or(|w| {
                        w.as_u64()
                            .is_some_and(|w| (100..=900).contains(&w) && w % 100 == 0)
                    })
                    && map
                        .keys()
                        .all(|key| key == "family" || key == "file" || key == "weight")
            }
            _ => false,
        },
        Grammar::Builtin => value
            .as_str()
            .is_some_and(|name| builtin_style_names().contains(&name)),
        Grammar::BrandColour => value.as_str().is_some_and(|color| {
            let lower = color.to_ascii_lowercase();
            chart_color(color)
                && !color.contains('/')
                && !lower.starts_with("rgba(")
                && !lower.starts_with("hsla(")
                // The legacy comma form's fourth value is an alpha too.
                && color.matches(',').count() <= 2
                && !color
                    .strip_prefix('#')
                    .is_some_and(|hex| matches!(hex.len(), 4 | 8))
        }),
        Grammar::BrandName => value.as_str().is_some_and(|name| {
            !name.trim().is_empty() && name.len() <= 200 && !name.chars().any(char::is_control)
        }),
        Grammar::BrandLogo => value.as_str().is_some_and(|path| {
            path.len() <= 200
                && !path.chars().any(char::is_control)
                && crate::folder::report_relative_path(path)
        }),
        Grammar::Step => value
            .as_str()
            .is_some_and(|word| super::settings::step_index(word).is_some()),
        Grammar::Preset => value.as_str().is_some_and(|name| {
            builtin_style_names().contains(&name) || super::presets::recipe(name).is_some()
        }),
    }
}

/// A report-relative path to a WOFF2 file that stays inside the report folder.
fn valid_font_file(path: &str) -> bool {
    path.len() <= 200
        && path.ends_with(".woff2")
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != ".." && part != ".")
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
}

fn grammar_name(grammar: Grammar) -> String {
    match grammar {
        Grammar::Colour => "a colour".into(),
        Grammar::Font => "a font stack of letters, digits, spaces and _ . , ' \" + -".into(),
        Grammar::Dimension => {
            "a non-negative number or a length in px, ch, rem, em, %, vh, vw, fr, s or ms".into()
        }
        Grammar::Shadow => {
            "`none` or `x y blur colour`, as in `0 2px 8px oklch(0 0 0 / 0.12)`".into()
        }
        Grammar::Number(low, high) => format!("a number from {low} to {high}"),
        Grammar::Boolean => "true or false".into(),
        Grammar::Enum(words) => format!("one of {}", words.join(", ")),
        Grammar::Weight => "normal, bold, lighter, bolder or a number from 1 to 1000".into(),
        Grammar::LetterSpacing => "`normal` or a signed length".into(),
        Grammar::Palette => format!(
            "a builtin palette ({}) or 1 to 8 colours",
            PALETTE_NAMES.join(", ")
        ),
        Grammar::Colours(n) => format!("exactly {n} colours"),
        Grammar::FontSetting => {
            "a font stack, or {family, file, weight?} with a .woff2 file in the report folder"
                .into()
        }
        Grammar::Builtin => format!("one of {}", builtin_style_names().join(", ")),
        Grammar::BrandColour => "an opaque colour".into(),
        Grammar::BrandName => "a non-empty company name".into(),
        Grammar::BrandLogo => "a report-relative image file".into(),
        Grammar::Step => super::settings::STEPS.join(", "),
        Grammar::Preset => format!("one of {}", super::presets::preset_names().join(", ")),
    }
}

struct Walk<'a> {
    index: usize,
    builtin: bool,
    diagnostics: &'a mut Vec<Diagnostic>,
    kept: Map<String, Value>,
}

impl Walk<'_> {
    fn at(&self, path: &str) -> String {
        format!("styles[{}].{path}", self.index)
    }

    fn report(&mut self, level: &str, code: &str, message: String, path: &str) {
        self.diagnostics
            .push(Diagnostic::new(level, code, message, self.at(path)));
    }

    fn keep(&mut self, path: &str, value: &Value) {
        let mut at = &mut self.kept;
        let mut parts = path.split('.').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                at.insert(part.to_owned(), value.clone());
            } else {
                let next = at
                    .entry(part.to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                at = next.as_object_mut().expect("kept branches are mappings");
            }
        }
    }

    fn walk(&mut self, path: &str, key: &str, value: &Value) {
        let full = if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}.{key}")
        };
        if let Some(leaf) = index().get(full.as_str()).copied() {
            // `motion` is a section and a setting: a mapping is the section. `background` is a
            // mapping, or `none`.
            if (full == "motion" || full == "background") && value.is_object() {
                self.branch(&full, value);
                return;
            }
            // One word stands for its mapping: `background: grain` is `{texture: grain}`.
            if full == "background"
                && let Some(map) = value.as_str().and_then(super::background::one_word)
            {
                self.branch(&full, &map);
                return;
            }
            let old_motion = full == "motion" && matches!(value.as_str(), Some("on" | "off"));
            if !self.builtin && (leaf.mark == Mark::Legacy || old_motion) {
                self.report_retired(&full, value);
            } else if leaf.mark == Mark::Internal && !self.builtin {
                self.report(
                    "warning",
                    "style.internal",
                    format!("'{full}' fixes the page's mechanics; only builtins set it. The leaf is skipped"),
                    &full,
                );
            } else if !accepts(leaf.grammar, value) {
                self.report(
                    "warning",
                    "style.schema",
                    format!(
                        "Invalid value for '{full}': expected {}. The leaf is skipped",
                        grammar_name(leaf.grammar)
                    ),
                    &full,
                );
            } else {
                if leaf.mark == Mark::Experimental && !self.builtin {
                    let message = format!(
                        "'{full}' is experimental: it can break the layout in ways nobody expects"
                    );
                    self.report("info", "style.experimental", message, &full);
                }
                self.keep(&full, value);
            }
        } else if let Some(reason) = retired(&full) {
            self.report(
                "warning",
                "style.retired",
                format!("Retired style leaf '{full}' is ignored: {reason}. Remove it."),
                &full,
            );
        } else if is_branch(&full) {
            if value.is_object() {
                self.branch(&full, value);
            } else {
                self.report(
                    "warning",
                    "style.schema",
                    format!("'{full}' must be a mapping. The leaf is skipped"),
                    &full,
                );
            }
        } else {
            let siblings: Vec<_> = index()
                .values()
                .filter(|leaf| leaf.mark == Mark::Safe)
                .map(|leaf| leaf.path.as_str())
                .filter(|known| {
                    known.rsplit_once('.').map(|(parent, _)| parent)
                        == full.rsplit_once('.').map(|(parent, _)| parent)
                })
                .collect();
            let near = crate::walker::nearest(&full, &siblings);
            let (message, fix) = match near {
                Some(near) => (
                    format!(
                        "Unknown style key '{full}'; did you mean '{near}'? The leaf is skipped"
                    ),
                    format!("Rename `{full}` to `{near}`, or remove it."),
                ),
                None => (
                    format!("Unknown style key '{full}'. The leaf is skipped"),
                    format!(
                        "Remove `{full}`; `richview learn style` lists the supported style fields."
                    ),
                ),
            };
            self.diagnostics.push(
                Diagnostic::new("warning", "style.unknown-key", message, self.at(&full)).fix(fix),
            );
        }
    }

    fn report_retired(&mut self, path: &str, value: &Value) {
        let replacement = match (path, value.as_str()) {
            ("motion", Some("off")) => Some("motion: xsmall"),
            ("motion", Some("on")) => Some("motion: medium"),
            _ => retired_replacement(path),
        };
        let advice = replacement.map_or_else(
            || "Remove it. The preset controls this value.".to_owned(),
            |setting| format!("Use `{setting}` instead."),
        );
        self.diagnostics.push(
            Diagnostic::new(
                "warning",
                "style.retired",
                format!("Retired style leaf '{path}' is ignored. {advice}"),
                self.at(path),
            )
            .fix(advice),
        );
    }

    fn branch(&mut self, path: &str, value: &Value) {
        if let Some(map) = value.as_object() {
            for (key, value) in map {
                self.walk(path, key, value);
            }
        }
    }
}

/// Check one style file against the vocabulary and return what survives, with the settings
/// expanded into their sections. Bad leaves are reported and left out; the rest still applies.
pub(super) fn load(
    value: &Map<String, Value>,
    index: usize,
    builtin: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Value {
    let mut walk = Walk {
        index,
        builtin,
        diagnostics,
        kept: Map::new(),
    };
    for (key, value) in value {
        walk.walk("", key, value);
    }
    expand(Value::Object(walk.kept))
}

/// Settings become the sections they stand for. The type scale and the accent depend on the
/// whole cascade, so they wait in `settings` until it is merged.
fn expand(doc: Value) -> Value {
    let Value::Object(mut doc) = doc else {
        return doc;
    };
    fn section<'a>(doc: &'a mut Map<String, Value>, name: &str) -> &'a mut Map<String, Value> {
        doc.entry(name)
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("sections are mappings")
    }
    if let Some(palette) = doc.remove("palette") {
        section(&mut doc, "chart").insert("palette".into(), palette);
    }
    for setting in super::settings::SETTINGS {
        if setting.name == "motion" && doc.get("motion").is_some_and(Value::is_object) {
            continue;
        }
        if let Some(Value::String(word)) = doc.remove(setting.name)
            && let Some(step) = super::settings::step_index(&word)
        {
            section(&mut doc, "settings")
                .entry("steps")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .expect("steps are a mapping")
                .insert(setting.name.into(), json!(step));
        }
    }
    match doc.remove("background") {
        Some(Value::Object(mut background)) => {
            if let Some(word) = background.get("strength").and_then(Value::as_str)
                && let Some(step) = super::settings::step_index(word)
            {
                background.insert("strength".into(), json!(step));
            }
            section(&mut doc, "settings").insert("background".into(), Value::Object(background));
        }
        // `background: none` clears an earlier file's background.
        Some(_) => {
            section(&mut doc, "settings").insert("background".into(), json!({}));
        }
        None => {}
    }
    if let Some(Value::Object(colours)) = doc.remove("colors") {
        section(&mut doc, "settings").insert("colors".into(), Value::Object(colours));
    }
    if let Some(Value::Object(fonts)) = doc.remove("fonts") {
        for (role, setting) in fonts {
            let family = match &setting {
                Value::Object(map) => map.get("family").cloned().unwrap_or_default(),
                other => other.clone(),
            };
            section(&mut doc, "tokens").insert(format!("font-{role}"), family);
            if let Value::Object(map) = &setting {
                let files = section(&mut doc, "settings")
                    .entry("brand-fonts")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .expect("brand fonts are a mapping");
                files.insert(role, Value::Object(map.clone()));
            }
        }
    }
    Value::Object(doc)
}
