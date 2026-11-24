//! Backgrounds behind a report (STY-74): a texture, a soft gradient from the author's colours,
//! or the author's own picture, on the page, in the margins or behind the header.
//!
//! Authors name a background only through closed words and a report-relative picture path;
//! every CSS value here is generated from validated numbers (INT-28, ARC-12, ARC-13). The
//! background yields to the text: each ink keeps its STY-25 floor over the background's worst
//! pixel. Inks the author did not set move in lightness first; only when they cannot does the
//! background weaken. Charts, tables and embeds keep solid surfaces, and code keeps its own opaque
//! panel (STY-61). Everything is screen only:
//! paper prints the flat page (STY-21).

use super::settings;
use crate::Diagnostic;
use richview_chart::color::{self, Oklch};
use serde_json::{Value, json};

/// The textures, `none` first.
pub const TEXTURES: [&str; 5] = ["none", "grain", "dots", "grid", "lines"];
/// The gradients, `none` first.
pub const GRADIENTS: [&str; 3] = ["none", "soft", "glow"];
/// Where a background sits.
pub const PLACES: [&str; 3] = ["page", "margins", "header"];
/// The words `background` takes alone: `none`, a texture or a gradient. A picture file is the
/// fourth one-word form, recognised by its extension.
pub const WORDS: [&str; 7] = ["none", "grain", "dots", "grid", "lines", "soft", "glow"];
/// The extensions that make one word a picture: the still rasters a background takes.
const PICTURE_EXTENSIONS: [&str; 4] = [".png", ".jpg", ".jpeg", ".webp"];

/// The mapping one word stands for (`background: grain`): a texture, a gradient, or a picture
/// file by its extension. `none` and any other word stand for no mapping.
pub(crate) fn one_word(word: &str) -> Option<Value> {
    if word == "none" {
        return None;
    }
    if TEXTURES.contains(&word) {
        return Some(json!({ "texture": word }));
    }
    if GRADIENTS.contains(&word) {
        return Some(json!({ "gradient": word }));
    }
    let lower = word.to_ascii_lowercase();
    PICTURE_EXTENSIONS
        .iter()
        .any(|extension| lower.ends_with(extension))
        .then(|| json!({ "picture": word }))
}

/// A texture's contrast over the page per step, for textures that text sits on. Grid and lines
/// draw continuous rules; grain and dots cover few pixels, so they need more to be present.
const LINE_STEPS: [f64; 5] = [1.04, 1.08, 1.14, 1.25, 1.40];
const GRAIN_STEPS: [f64; 5] = [1.10, 1.18, 1.28, 1.42, 1.60];
const DOT_STEPS: [f64; 5] = [1.08, 1.15, 1.25, 1.40, 1.60];
/// In the margins no text sits on the texture, so it may be stronger.
const MARGIN_STEPS: [f64; 5] = [1.08, 1.16, 1.30, 1.50, 1.80];
/// A gradient's stop contrast over the page per step.
const GRADIENT_STEPS: [f64; 5] = [1.06, 1.12, 1.20, 1.32, 1.45];
/// How much of the way from the least wash to a solid page each step adds over a picture.
const WASH_SPREAD: [f64; 5] = [0.7, 0.5, 0.3, 0.15, 0.0];
/// The most chroma a gradient stop adds to the page.
const GRADIENT_CHROMA: f64 = 0.035;

/// The inks that can sit on a page background, with their STY-25 floors: text, links, the
/// accent, ticks, and the status colours of deltas and callout titles. Axis text and series sit
/// on the chart's own solid surface. `brand`, the wordmark's shade, is not drawn on a report's
/// page (the colophon sets RichView in `ink-soft` and `link`), so it is neither checked nor moved
/// here (STY-01).
pub(crate) const FLOORS: [(&str, f64); 8] = [
    ("ink", 7.0),
    ("ink-soft", 4.5),
    ("link", 4.5),
    ("accent", 3.0),
    ("ink-faint", 3.0),
    ("up", 4.5),
    ("down", 4.5),
    ("hold", 4.5),
];

/// A cascade's background, read from `settings.background`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Spec {
    pub texture: Option<String>,
    pub gradient: Option<String>,
    pub picture: Option<String>,
    pub place: String,
    pub step: usize,
}

impl Spec {
    pub(crate) fn read(document: &Value) -> Option<Self> {
        let background = document.pointer("/settings/background")?;
        let word = |key: &str| {
            background
                .get(key)
                .and_then(Value::as_str)
                .filter(|word| *word != "none")
                .map(str::to_owned)
        };
        let picture = word("picture");
        // A picture stands alone (`style.background-conflict` says so at load).
        let (texture, gradient) = if picture.is_some() {
            (None, None)
        } else {
            (word("texture"), word("gradient"))
        };
        if texture.is_none() && gradient.is_none() && picture.is_none() {
            return None;
        }
        let place = background
            .get("place")
            .and_then(Value::as_str)
            .unwrap_or(if picture.is_some() { "header" } else { "page" })
            .to_owned();
        let step = background
            .get("strength")
            .and_then(Value::as_u64)
            .map_or(settings::MEDIUM, |s| (s as usize).min(4));
        Some(Self {
            texture,
            gradient,
            picture,
            place,
            step,
        })
    }

    fn text_on_it(&self) -> bool {
        self.place != "margins"
    }
}

fn token(document: &Value, name: &str) -> Option<Oklch> {
    document
        .pointer(&format!("/tokens/{name}"))
        .and_then(Value::as_str)
        .and_then(color::parse)
}

/// The page moved in lightness toward the text, keeping its hue, until `target` contrast.
fn toward(page: Oklch, dark: bool, chroma: f64, hue: f64, target: f64) -> Oklch {
    let mut candidate = Oklch::new(page.l, chroma, hue);
    while color::contrast(candidate, page) < target && (0.0..=1.0).contains(&candidate.l) {
        candidate.l = if dark {
            (candidate.l + 0.002).min(1.0)
        } else {
            (candidate.l - 0.002).max(0.0)
        };
        if candidate.l <= 0.0 || candidate.l >= 1.0 {
            break;
        }
    }
    candidate
}

/// A colour as the CSS writes it: `Oklch::css` rounds, and the floors hold for what is written.
fn written(colour: Oklch) -> Oklch {
    color::parse(&colour.css()).unwrap_or(colour)
}

/// An ink moved in lightness, away from the page, until it reaches `floor` over every colour of
/// `worst`; `None` when no lightness reaches it.
fn reach(ink: Oklch, dark: bool, floor: f64, worst: &[Oklch]) -> Option<Oklch> {
    let mut moved = ink;
    loop {
        if worst
            .iter()
            .all(|w| color::contrast(written(moved), *w) >= floor)
        {
            return Some(written(moved));
        }
        if (dark && moved.l >= 1.0) || (!dark && moved.l <= 0.0) {
            return None;
        }
        moved.l = if dark {
            (moved.l + 0.005).min(1.0)
        } else {
            (moved.l - 0.005).max(0.0)
        };
    }
}

/// What the derivation settled on.
#[derive(Debug, Default)]
struct Settled {
    texture: Option<Oklch>,
    stop: Option<Oklch>,
    moved: Vec<(&'static str, Oklch)>,
    weakened: bool,
}

/// Derive the background's colours from the final inks and write them to
/// `settings.background.derived`; move the inks the author did not set where the background
/// needs it. Run after the colour derivation, so `tint`, `gridlines` and `colors` are settled.
/// `preset_default` says the background is the preset's own (no author file or frontmatter named
/// one): it then yields to every ink the preset set, so the preset keeps its exact colours, and
/// weakens without a finding, since the author asked for nothing.
pub(super) fn derive(
    merged: &mut Value,
    at: &str,
    preset_default: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(spec) = Spec::read(merged) else {
        return;
    };
    let Some(page) = token(merged, "bg") else {
        return;
    };
    let ink = token(merged, "ink").unwrap_or(Oklch::new(0.2, 0.0, 0.0));
    let dark = page.luminance() < ink.luminance();
    let inks: Vec<(&'static str, f64, Oklch)> = FLOORS
        .iter()
        .filter_map(|(name, floor)| token(merged, name).map(|c| (*name, *floor, c)))
        .collect();
    // The hue a gradient carries: the author's brand (or, for a glow, the second colour), else
    // the preset's accent.
    let hue_of = |key: &str| {
        merged
            .pointer(&format!("/settings/colors/{key}"))
            .and_then(Value::as_str)
            .and_then(color::parse)
    };
    let accent = token(merged, "accent").unwrap_or(page);
    let tone = match spec.gradient.as_deref() {
        Some("glow") => hue_of("secondary").or_else(|| hue_of("brand")),
        _ => hue_of("brand"),
    }
    .unwrap_or(accent);
    let texture_steps = if spec.place == "margins" {
        MARGIN_STEPS
    } else {
        match spec.texture.as_deref() {
            Some("grain") => GRAIN_STEPS,
            Some("dots") => DOT_STEPS,
            _ => LINE_STEPS,
        }
    };
    // Try the step's strength, then weaker, until every ink can reach its floor.
    let full = (
        spec.texture
            .as_ref()
            .map(|_| texture_steps[spec.step] - 1.0),
        spec.gradient
            .as_ref()
            .map(|_| GRADIENT_STEPS[spec.step] - 1.0),
    );
    let mut settled = Settled::default();
    for tenth in (0..=20).rev() {
        let scale = f64::from(tenth) / 20.0;
        let texture = full
            .0
            .map(|gain| written(toward(page, dark, page.c, page.h, 1.0 + gain * scale)));
        let stop = full.1.map(|gain| {
            let chroma = page.c + (tone.c * 0.3).min(GRADIENT_CHROMA);
            written(toward(page, dark, chroma, tone.h, 1.0 + gain * scale))
        });
        let mut worst: Vec<Oklch> = texture.into_iter().collect();
        if let Some(stop) = stop {
            worst.extend((0..=16).map(|i| color::mix(page, stop, f64::from(i) / 16.0)));
        }
        if !spec.text_on_it() || worst.is_empty() {
            settled = Settled {
                texture,
                stop,
                ..Settled::default()
            };
            break;
        }
        let mut moved = vec![];
        let ok = inks.iter().all(|(name, floor, ink)| {
            if worst.iter().all(|w| color::contrast(*ink, *w) >= *floor) {
                return true;
            }
            if preset_default {
                return false;
            }
            match reach(*ink, dark, *floor, &worst) {
                Some(new) => {
                    moved.push((*name, new));
                    true
                }
                None => false,
            }
        });
        if ok || tenth == 0 {
            settled = Settled {
                texture,
                stop,
                moved: if ok { moved } else { vec![] },
                weakened: tenth < 20,
            };
            break;
        }
    }
    for (name, ink) in &settled.moved {
        merged["tokens"][*name] = json!(ink.css());
    }
    // Text on the accent follows a moved accent, where the author's colours chose it.
    if settled.moved.iter().any(|(name, _)| *name == "accent")
        && merged.pointer("/tokens/on-accent").is_some()
    {
        super::on_accent(merged);
    }
    let mut derived = json!({});
    if let Some(texture) = settled.texture {
        derived["texture"] = json!(texture.css());
    }
    if let Some(stop) = settled.stop {
        derived["stop"] = json!(stop.css());
    }
    if spec.picture.is_some() && spec.place == "page" {
        derived["wash"] = json!(wash(merged, page, spec.step));
    }
    merged["settings"]["background"]["derived"] = derived;
    if settled.weakened && !preset_default {
        diagnostics.push(
            Diagnostic::new(
                "info",
                "style.background-adjusted",
                "The background was made fainter so that text keeps its contrast over it",
                format!("{at}.strength"),
            )
            .fix("Keep it, or choose a lower strength or a lighter page colour"),
        );
    }
}

/// The least opacity of a page-coloured wash over any picture that keeps every ink's floor,
/// spread toward a solid page by the step. Every pixel of a picture lies between black and
/// white, so the composite over both extremes bounds every pixel (CSS composites in sRGB).
fn wash(document: &Value, page: Oklch, step: usize) -> f64 {
    let inks: Vec<(f64, Oklch)> = FLOORS
        .iter()
        .filter_map(|(name, floor)| token(document, name).map(|c| (*floor, c)))
        .collect();
    let rgb = page.srgb();
    let holds = |alpha: f64| {
        [0.0, 1.0].iter().all(|extreme| {
            let composite = Oklch::from_srgb(rgb.map(|p| alpha * p + (1.0 - alpha) * extreme));
            inks.iter()
                .all(|(floor, ink)| color::contrast(*ink, composite) >= *floor)
        })
    };
    let mut alpha = 0.0;
    while alpha < 1.0 && !holds(alpha) {
        alpha += 0.005;
    }
    let alpha = alpha.min(1.0);
    let spread = alpha + (1.0 - alpha) * WASH_SPREAD[step.min(4)];
    (spread * 1000.0).round() / 1000.0
}

/// The solid surfaces figures keep over a page background.
const SOLID: &str = ".rv-chart,.rv-table,.rv-table-scroll,.rv-embed{background:var(--rv-bg-chart);box-shadow:0 0 0 .75rem var(--rv-bg-chart);border-radius:var(--rv-radius-s,0)}";
/// High contrast and forced colours show the flat page.
const PLAIN: &str = "@media (prefers-contrast:more),(forced-colors:active){html{background:var(--rv-bg)!important}.rv-report__header::before,.rv-report__header::after,body::before{display:none}}\n";
/// The sheet the report sits on when its background is in the margins.
const SHEET: &str = ".rv-report{--rv-column-w:calc(min(var(--rv-report-max-width,980px),100vw) - 6rem);background:var(--rv-bg);padding:3rem 3rem 4rem;margin:72px auto;border-radius:var(--rv-radius-m,0)}@media (max-width:1040px){.rv-report{--rv-column-w:calc(100vw - 3rem);margin:6rem 0 0;max-width:none;border-radius:0;padding:2rem 1.5rem 4rem}}@media (max-width:640px){.rv-report{--rv-column-w:calc(100vw - 2rem);padding:1.5rem 1rem 2rem}}";

/// The texture's CSS background layers, drawn in `ink`.
fn texture_layers(texture: &str, ink: &str, centred: bool) -> Vec<String> {
    let x = if centred { "50%" } else { "0" };
    match texture {
        "grain" => {
            let rgb = color::parse(ink).map_or([0.0; 3], Oklch::srgb);
            let svg = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='240' height='240'><filter id='g' color-interpolation-filters='sRGB'><feTurbulence type='fractalNoise' baseFrequency='.8' numOctaves='3' stitchTiles='stitch'/><feColorMatrix values='0 0 0 0 {:.4} 0 0 0 0 {:.4} 0 0 0 0 {:.4} 2.2 0 0 0 -.75'/></filter><rect width='240' height='240' filter='url(#g)'/></svg>",
                rgb[0], rgb[1], rgb[2]
            );
            // Base64, so the filter's own `url(#g)` never reads as a CSS url() of the page.
            use base64::{Engine, engine::general_purpose::STANDARD};
            let encoded = STANDARD.encode(svg);
            vec![format!(
                "url(\"data:image/svg+xml;base64,{encoded}\") 0 0/240px 240px"
            )]
        }
        "dots" => vec![format!(
            "radial-gradient(circle,{ink} 1.3px,transparent 1.8px) {x} 0/18px 18px"
        )],
        "grid" => vec![
            format!("linear-gradient(to top,{ink} 1px,transparent 1px) {x} 0/24px 24px"),
            format!("linear-gradient(90deg,{ink} 1px,transparent 1px) {x} 0/24px 24px"),
        ],
        "lines" => vec![format!(
            "linear-gradient(to top,{ink} 1px,transparent 1px) 0 0/100% calc(var(--rv-size-base) * var(--rv-line-base))"
        )],
        _ => vec![],
    }
}

/// The gradient's CSS layer; `band` fits it to the header band instead of the first screen.
fn gradient_layer(gradient: &str, stop: &str, page: &str, band: bool) -> String {
    match (gradient, band) {
        ("glow", false) => format!(
            "radial-gradient(in oklch circle at 88% 0,{stop},{page} min(60vw,100vh)) 0 0/100% 100vh no-repeat"
        ),
        ("glow", true) => format!("radial-gradient(in oklch circle at 88% 0,{stop},{page} 70%)"),
        (_, false) => {
            format!(
                "linear-gradient(in oklch to bottom,{stop},{page} 70vh) 0 0/100% 100vh no-repeat"
            )
        }
        (_, true) => format!("linear-gradient(in oklch to bottom,{stop},{page})"),
    }
}

/// The CSS of a texture or gradient background, screen only. Empty without one, so a report
/// without a background is byte for byte as before. A picture's CSS waits for its bytes
/// (`picture_css`).
pub(super) fn css(document: &Value) -> String {
    let Some(spec) = Spec::read(document) else {
        return String::new();
    };
    if spec.picture.is_some() {
        return String::new();
    }
    let derived = &document["settings"]["background"]["derived"];
    let page = token(document, "bg").map_or_else(|| "var(--rv-bg)".to_owned(), Oklch::css);
    let mut layers = vec![];
    if let (Some(texture), Some(ink)) = (&spec.texture, derived["texture"].as_str()) {
        layers.extend(texture_layers(texture, ink, spec.place == "margins"));
    }
    let band = spec.place == "header";
    if let (Some(gradient), Some(stop)) = (&spec.gradient, derived["stop"].as_str()) {
        layers.push(gradient_layer(gradient, stop, &page, band));
    }
    if layers.is_empty() {
        return String::new();
    }
    let layers = layers.join(",");
    let mut css = String::from("@media screen{");
    match spec.place.as_str() {
        "header" => css.push_str(&format!(
            "body{{overflow-x:clip}}.rv-report__header{{position:relative;isolation:isolate;margin-bottom:var(--rv-line)}}.rv-report__header::before{{content:\"\";position:absolute;z-index:-1;top:-100vh;bottom:calc(-1 * var(--rv-line));left:calc(50% - 50vw);width:100vw;background:{layers},{page}}}"
        )),
        "margins" => {
            css.push_str(&format!(
                "html{{background:{layers},{page}}}body{{background:transparent}}"
            ));
            css.push_str(SHEET);
        }
        _ => {
            css.push_str(&format!(
                "html{{background:{layers},{page}}}body{{background:transparent}}"
            ));
            css.push_str(SOLID);
        }
    }
    css.push_str("}\n");
    css.push_str(PLAIN);
    css
}

/// The CSS of a validated background picture, screen only.
pub(crate) fn picture_css(document: &Value, uri: &str, width: u32, height: u32) -> String {
    let Some(spec) = Spec::read(document) else {
        return String::new();
    };
    let page = token(document, "bg").map_or_else(|| "var(--rv-bg)".to_owned(), Oklch::css);
    let image = format!("url(\"{uri}\")");
    let mut css = String::from("@media screen{");
    match spec.place.as_str() {
        "page" => {
            let alpha = document["settings"]["background"]["derived"]["wash"]
                .as_f64()
                .unwrap_or(1.0);
            let veil = token(document, "bg").map_or_else(
                || "var(--rv-bg)".to_owned(),
                |c| format!("oklch({:.4} {:.4} {:.2} / {alpha:.3})", c.l, c.c, c.h),
            );
            let ratio = f64::from(width) / f64::from(height.max(1));
            css.push_str(&format!(
                "html{{background:linear-gradient(to bottom,{veil} 0,{veil} 55vh,{page} 100vh) 0 0/100% 100% no-repeat,{image} 50% 0/max(100%,calc(100vh * {ratio:.4})) auto no-repeat,{page}}}body{{background:transparent}}{SOLID}"
            ));
        }
        "margins" => {
            css.push_str(&format!(
                "html{{background:{page}}}body{{background:transparent}}body::before{{content:\"\";position:fixed;inset:0;z-index:-1;background:{image} 50% 50%/cover no-repeat}}{SHEET}"
            ));
        }
        _ => {
            css.push_str(&format!(
                "body{{overflow-x:clip}}.rv-report__header{{position:relative;isolation:isolate;margin-top:7rem;margin-bottom:2.5rem;padding:1.75rem 2.25rem 1.25rem;max-width:calc(var(--rv-measure-w) + 4.5rem)}}.rv-report__header::after{{content:\"\";position:absolute;z-index:-1;inset:0;background:var(--rv-bg);border-radius:var(--rv-radius-m,0)}}.rv-report__header::before{{content:\"\";position:absolute;z-index:-1;top:-100vh;bottom:-2.5rem;left:calc(50% - 50vw);width:100vw;background:{image} 50% 100%/cover no-repeat}}@media (max-width:640px){{.rv-report__header{{margin-top:calc(min(56.25vw,40vh) - 1.5rem);margin-bottom:0;padding:1.25rem 0 0}}.rv-report__header::after{{border-radius:0}}.rv-report__header::before{{top:calc(-1 * min(56.25vw,40vh));bottom:auto;height:min(56.25vw,40vh);background-position:50% 50%}}}}"
            ));
        }
    }
    css.push_str("}\n");
    css.push_str(PLAIN);
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_strength_table_rises_and_starts_visible() {
        for table in [
            LINE_STEPS,
            GRAIN_STEPS,
            DOT_STEPS,
            MARGIN_STEPS,
            GRADIENT_STEPS,
        ] {
            assert!(table[0] > 1.0);
            assert!(table.windows(2).all(|pair| pair[0] < pair[1]), "{table:?}");
        }
        assert!(WASH_SPREAD.windows(2).all(|pair| pair[0] > pair[1]));
    }

    #[test]
    fn the_wash_holds_every_floor_over_black_and_white() {
        let document = json!({"tokens": {
            "bg": "oklch(0.99 0.004 70)", "ink": "oklch(0.2 0 0)", "ink-soft": "oklch(0.45 0 0)",
            "link": "oklch(0.5 0.15 260)", "accent": "oklch(0.5 0.15 260)", "ink-faint": "oklch(0.6 0 0)"
        }});
        let page = token(&document, "bg").unwrap();
        let alpha = wash(&document, page, 4);
        assert!(alpha > 0.5 && alpha < 1.0, "{alpha}");
        for extreme in [0.0, 1.0] {
            let composite =
                Oklch::from_srgb(page.srgb().map(|p| alpha * p + (1.0 - alpha) * extreme));
            // As `wash` does, a floor whose ink the document does not set holds nothing.
            for (name, floor) in FLOORS {
                let Some(ink) = token(&document, name) else {
                    continue;
                };
                assert!(
                    color::contrast(ink, composite) >= floor,
                    "{name} over {extreme}"
                );
            }
        }
        assert!(wash(&document, page, 0) > alpha);
    }

    /// A light page whose status inks sit just above their 4.5:1 floor, so that a texture or a
    /// picture under them breaks it unless the derivation holds them.
    fn status_document(background: Value) -> Value {
        json!({
            "tokens": {
                "bg": "oklch(0.99 0.004 70)", "ink": "oklch(0.2 0 0)", "ink-soft": "oklch(0.45 0 0)",
                "link": "oklch(0.5 0.15 260)", "accent": "oklch(0.5 0.15 260)",
                "ink-faint": "oklch(0.6 0 0)", "up": "oklch(0.52 0.14 150)",
                "down": "oklch(0.55 0.19 25)", "hold": "oklch(0.53 0.02 260)"
            },
            "settings": {"background": background}
        })
    }

    const STATUS: [&str; 3] = ["up", "down", "hold"];

    #[test]
    fn status_inks_keep_their_floor_over_the_strongest_textures() {
        for texture in ["grain", "dots"] {
            let before = status_document(json!({"texture": texture, "strength": 4}));
            let page = token(&before, "bg").unwrap();
            // The inks hold on the flat page and would not over the texture at full strength.
            let strongest = if texture == "grain" {
                GRAIN_STEPS[4]
            } else {
                DOT_STEPS[4]
            };
            let full = written(toward(page, false, page.c, page.h, strongest));
            for name in STATUS {
                let ink = token(&before, name).unwrap();
                assert!(color::contrast(ink, page) >= 4.5, "{name} on the page");
                assert!(color::contrast(ink, full) < 4.5, "{name} must be at risk");
            }
            // Inks the author did not set move, and the texture keeps its full strength.
            let (mut moved, mut findings) = (before.clone(), vec![]);
            derive(&mut moved, "background", false, &mut findings);
            assert!(findings.is_empty(), "{texture}: {findings:?}");
            let drawn = token_at(&moved, "/settings/background/derived/texture");
            assert!(
                color::contrast(drawn, page) >= strongest - 0.01,
                "{texture}"
            );
            for name in STATUS {
                let ink = token(&moved, name).unwrap();
                assert!(color::contrast(ink, drawn) >= 4.5, "{texture}: {name}");
                assert_ne!(moved["tokens"][name], before["tokens"][name], "{name}");
            }
            // A preset’s own texture yields to its inks without an author warning.
            let (mut kept, mut findings) = (before.clone(), vec![]);
            derive(&mut kept, "background", true, &mut findings);
            assert!(findings.is_empty(), "{texture}: {findings:?}");
            let drawn = token_at(&kept, "/settings/background/derived/texture");
            for name in STATUS {
                assert_eq!(kept["tokens"][name], before["tokens"][name], "{name}");
                let ink = token(&kept, name).unwrap();
                assert!(color::contrast(ink, drawn) >= 4.5, "{texture}: {name}");
            }
        }
    }

    #[test]
    fn status_inks_keep_their_floor_under_a_picture_on_the_page() {
        let mut document =
            status_document(json!({"picture": "hero.jpg", "place": "page", "strength": 4}));
        derive(&mut document, "background", false, &mut vec![]);
        let alpha = document["settings"]["background"]["derived"]["wash"]
            .as_f64()
            .unwrap();
        let page = token(&document, "bg").unwrap();
        for extreme in [0.0, 1.0] {
            let composite =
                Oklch::from_srgb(page.srgb().map(|p| alpha * p + (1.0 - alpha) * extreme));
            for name in STATUS {
                let ink = token(&document, name).unwrap();
                assert!(
                    color::contrast(ink, composite) >= 4.5,
                    "{name} over {extreme}"
                );
            }
        }
        // The status inks bind: without them the same page needs less wash.
        let mut without = document.clone();
        for name in STATUS {
            without["tokens"].as_object_mut().unwrap().remove(name);
        }
        assert!(wash(&without, page, 4) < alpha);
    }

    fn token_at(document: &Value, pointer: &str) -> Oklch {
        document
            .pointer(pointer)
            .and_then(Value::as_str)
            .and_then(color::parse)
            .unwrap()
    }
}
