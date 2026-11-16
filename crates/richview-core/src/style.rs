use crate::{Diagnostic, text};
use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::OnceLock};

mod brand;
pub mod checks;
mod components;
pub(crate) use components::frame_inset_px;
mod vocabulary;
pub use vocabulary::{Grammar, Leaf, Mark, leaves as style_vocabulary};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedStyle {
    /// `dark` or `light` (STY-57); reports emit it as `color-scheme`.
    pub scheme: String,
    pub document: Value,
    pub custom_properties: BTreeMap<String, String>,
    pub chart: Value,
    pub css: String,
    pub font_css: String,
}

/// The five builtins in the order of STY-07. `sleek` is the default (STY-06).
pub fn builtin_style_names() -> &'static [&'static str] {
    &["sleek", "air", "scientific", "brutalist", "hand-drawn"]
}

/// The builtin every cascade starts from when it names none (STY-17).
pub const DEFAULT_STYLE: &str = "sleek";

/// The replacement for a retired builtin name (TYP-52); the migration rewrites these.
fn renamed(name: &str) -> Option<&'static str> {
    crate::registry::RETIRED_STYLES
        .iter()
        .find(|(old, _)| *old == name)
        .map(|(_, replacement)| *replacement)
}

/// Whether `name` is a retired builtin name, which is a `style.renamed` error.
pub fn is_retired_style_name(name: &str) -> bool {
    renamed(name).is_some()
}

/// Whether a `styles` entry is a style name rather than a file or YAML text: one word, without a
/// path separator, an extension, a colon or a line break. Hosts read such an entry as a builtin
/// name and never as a file, so a misspelt one is `style.unknown`, not a missing asset.
pub fn is_style_name(reference: &str) -> bool {
    !reference.contains(['/', '.', ':', '\n'])
}

fn builtin(name: &str) -> Option<&'static str> {
    match name {
        "brutalist" => Some(include_str!("../assets/brutalist.rstyle.yaml")),
        "scientific" => Some(include_str!("../assets/scientific.rstyle.yaml")),
        "sleek" => Some(include_str!("../assets/sleek.rstyle.yaml")),
        "air" => Some(include_str!("../assets/air.rstyle.yaml")),
        "hand-drawn" => Some(include_str!("../assets/hand-drawn.rstyle.yaml")),
        _ => None,
    }
}

/// Declarative sources for generated host compatibility bindings.
pub fn builtin_style_source(name: &str) -> Option<&'static str> {
    builtin(name)
}

pub fn resolve_styles_compat(
    refs: &Value,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Value, Vec<Diagnostic>> {
    let refs = legacy_style_refs(refs)?;
    let resolved = resolve_styles(&refs, diagnostics);
    Ok(
        json!({"doc":resolved.document,"customProperties":resolved.custom_properties,"chart":resolved.chart,"motion":resolved.document.get("motion").unwrap_or(&json!({})),"components":resolved.document.get("components").unwrap_or(&json!({}))}),
    )
}

pub(crate) fn legacy_style_refs(refs: &Value) -> Result<Vec<String>, Vec<Diagnostic>> {
    let Some(refs) = refs.as_array() else {
        return Err(vec![Diagnostic::new(
            "error",
            "options.style",
            "Styles must be an ordered array",
            "options.styles",
        )]);
    };
    refs.iter().enumerate().map(|(i, style)| {
        style.as_str().or_else(||match style.get("kind").and_then(Value::as_str) {
            Some("builtin")=>style.get("name").and_then(Value::as_str),
            Some("source")=>style.get("source").and_then(Value::as_str),
            _=>None,
        }).map(str::to_owned).ok_or_else(||vec![Diagnostic::new("error","options.style","Style refs must be builtin names or {kind:builtin,name}/{kind:source,source}",format!("options.styles[{i}]"))])
    }).collect()
}

/// Serialize generated style properties without opening a second CSS injection boundary.
pub fn custom_properties_to_css(props: &Value) -> Result<String, Vec<Diagnostic>> {
    let Some(props) = props.as_object() else {
        return Err(vec![Diagnostic::new(
            "error",
            "style.properties",
            "Custom properties must be a mapping",
            "customProperties",
        )]);
    };
    if props.is_empty() {
        return Ok(String::new());
    }
    let mut doc = serde_json::Map::new();
    let mut values = BTreeMap::new();
    let mut diagnostics = vec![];
    for (key, value) in props {
        let Some(name) = key.strip_prefix("--rv-") else {
            diagnostics.push(Diagnostic::new(
                "error",
                "style.properties",
                "Only --rv- style properties are supported",
                key,
            ));
            continue;
        };
        let section = if name == "report-max-width" {
            "layout"
        } else {
            "tokens"
        };
        values.insert(key.clone(), text(value));
        doc.entry(section)
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("sections are mappings")
            .insert(name.to_owned(), value.clone());
    }
    // Hosts hand in resolved properties, internal ones included.
    vocabulary::load(&doc, 0, true, &mut diagnostics);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    Ok(token_css(&values))
}

/// Custom properties for resolved tokens. A `-phone` token replaces its base token at
/// phone widths, so every consumer of `--rv-size-h2` follows the narrow scale.
/// The token a chart readout's border takes instead of the hairline. A panel with no shadow is
/// told from the chart by its border alone, so it takes whichever of `rule` and `ink-faint`
/// contrasts more with the panel's fill; a panel with a shadow keeps the hairline (STY-41 as
/// assumed).
fn readout_edge(props: &BTreeMap<String, String>) -> Option<&'static str> {
    use richview_chart::color;
    let get = |name: &str| props.get(name).map(String::as_str);
    if get("--rv-shadow-float").is_some_and(|shadow| shadow.trim() != "none") {
        return None;
    }
    let panel = color::parse(get("--rv-bg-elev").or(get("--rv-bg"))?)?;
    let rule = color::parse(get("--rv-rule")?)?;
    let faint = color::parse(get("--rv-ink-faint")?)?;
    (color::contrast(faint, panel) > color::contrast(rule, panel)).then_some("--rv-ink-faint")
}

fn token_css(props: &BTreeMap<String, String>) -> String {
    let (phone, wide): (Vec<_>, Vec<_>) = props
        .iter()
        .partition(|(key, _)| key.ends_with(PHONE_SUFFIX));
    let block = |props: Vec<(&String, &String)>| {
        props
            .iter()
            .map(|(key, value)| {
                let key = key.strip_suffix(PHONE_SUFFIX).unwrap_or(key);
                format!("  {key}: {value};\n")
            })
            .collect::<String>()
    };
    let mut css = format!(":root {{\n{}}}\n", block(wide));
    if !phone.is_empty() {
        css.push_str(&format!(
            "@media (max-width: 640px) {{\n:root {{\n{}}}\n}}\n",
            block(phone)
        ));
    }
    css
}

const PHONE_SUFFIX: &str = "-phone";

/// Characters no style grammar admits. A sink refuses a value that has one instead of
/// escaping it, so a refusal points at a grammar that is missing (TYP-51).
pub(crate) fn sink_refuses(value: &str) -> bool {
    value.contains([';', '{', '}', '<', '>', '\\', '@'])
        || value.to_ascii_lowercase().contains("url(")
}

/// The typed serializer's last check: drop any property whose value a sink refuses and fail
/// the render with an internal error that names the key.
fn refuse_unsafe(props: &mut BTreeMap<String, String>, diagnostics: &mut Vec<Diagnostic>) {
    props.retain(|key, value| {
        let refused = sink_refuses(value);
        if refused {
            diagnostics.push(Diagnostic::new(
                "error",
                "style.sink",
                format!("Internal error: the value of '{key}' holds characters CSS output refuses; its grammar admits too much"),
                key.clone(),
            ));
        }
        !refused
    });
}

/// Tokens that steer the engine and are not CSS custom properties: the colour scheme and the
/// builtin a dark style prints as (STY-57).
const INTERNAL_TOKENS: [&str; 2] = ["scheme", "print-as"];

fn builtin_document(name: &str) -> Option<&'static Value> {
    static BUILTINS: OnceLock<BTreeMap<&'static str, Value>> = OnceLock::new();
    BUILTINS
        .get_or_init(|| {
            builtin_style_names()
                .iter()
                .map(|name| {
                    let doc =
                        crate::yaml::from_str(builtin(name).unwrap()).expect("bundled style YAML");
                    let mut diagnostics = vec![];
                    let doc = vocabulary::load(
                        doc.as_object().expect("bundled style mapping"),
                        0,
                        true,
                        &mut diagnostics,
                    );
                    assert!(
                        diagnostics.is_empty(),
                        "Bundled style {name} must satisfy its vocabulary: {diagnostics:?}"
                    );
                    (*name, doc)
                })
                .collect()
        })
        .get(name)
}

/// The cascade with the default builtin underneath when it names no builtin (STY-17): an
/// author's file then changes only what it names.
pub fn cascade_refs(refs: &[String]) -> Vec<String> {
    let names_builtin = refs
        .iter()
        .any(|reference| builtin_style_names().contains(&reference.as_str()));
    if names_builtin {
        return refs.to_vec();
    }
    std::iter::once(DEFAULT_STYLE.to_owned())
        .chain(refs.iter().cloned())
        .collect()
}

/// Move the `styles[n]` paths of diagnostics from `start` on down by `offset`, so a default
/// builtin placed underneath the author's list does not renumber it.
pub(crate) fn shift_style_paths(diagnostics: &mut [Diagnostic], offset: usize) {
    if offset == 0 {
        return;
    }
    for diagnostic in diagnostics {
        let Some(rest) = diagnostic.path.strip_prefix("styles[") else {
            continue;
        };
        if let Some((index, tail)) = rest.split_once(']')
            && let Ok(index) = index.parse::<usize>()
        {
            diagnostic.path = format!("styles[{}]{tail}", index.saturating_sub(offset));
        }
    }
}

pub fn resolve_styles(refs: &[String], diagnostics: &mut Vec<Diagnostic>) -> ResolvedStyle {
    let first_diagnostic = diagnostics.len();
    let mut cascaded = cascade_refs(refs);
    let offset = cascaded.len() - refs.len();
    // An unbased company style starts on Air only when the author named no builtin. Selecting
    // that base before the cascade preserves any earlier file layers.
    if offset > 0
        && refs.iter().any(|source| {
            crate::yaml::from_str(source).ok().is_some_and(|value| {
                value.pointer("/company/color").is_some()
                    && value.pointer("/company/base").is_none()
            })
        })
    {
        cascaded[0] = "air".into();
    }
    let refs = &cascaded;
    let mut merged = json!({});
    for (index, reference) in refs.iter().enumerate() {
        let name = reference.as_str();
        if let Some(replacement) = renamed(name) {
            diagnostics.push(Diagnostic::new(
                "error",
                "style.renamed",
                format!("Style '{name}' was renamed '{replacement}'; use '{replacement}'"),
                format!("styles[{index}]"),
            ));
            continue;
        }
        // Canonical declarative assets are validated once when first used. Warm live edits
        // should not parse the same builtin YAML on every keystroke.
        if let Some(value) = builtin_document(name) {
            merge_layer(&mut merged, value);
            continue;
        }
        let source = match builtin(name) {
            Some(src) => src,
            None if reference.contains(':')
                || reference.contains('\n')
                || reference.trim().is_empty() =>
            {
                reference
            }
            None => {
                let known = builtin_style_names();
                let mut message = format!(
                    "Style '{reference}' is not a builtin ({}) or a style file",
                    known.join(", ")
                );
                let fix = match crate::walker::nearest(reference, known) {
                    Some(near) => {
                        message.push_str(&format!("; did you mean '{near}'?"));
                        format!(
                            "Write `{near}`, or name a `.rstyle.yaml` file in the report folder."
                        )
                    }
                    None => {
                        message.push('.');
                        "Write a builtin name, or name a `.rstyle.yaml` file in the report folder."
                            .to_owned()
                    }
                };
                diagnostics.push(
                    Diagnostic::new(
                        "error",
                        "style.unknown",
                        message,
                        format!("styles[{index}]"),
                    )
                    .fix(fix)
                    .expected(known.join(", ")),
                );
                continue;
            }
        };
        let root = format!("styles[{index}]");
        match crate::yaml::load(source) {
            Ok(loaded) if loaded.value.is_null() => {}
            Ok(loaded) => {
                let before = diagnostics.len();
                let Some(map) = loaded.value.as_object() else {
                    diagnostics.push(Diagnostic::new(
                        "warning",
                        "style.type",
                        "Style must be a YAML mapping",
                        root.clone(),
                    ));
                    continue;
                };
                // A bad leaf is reported and left out; the rest of the file still applies.
                let value = vocabulary::load(map, index, false, diagnostics);
                crate::parser::place(&mut diagnostics[before..], &root, &loaded.positions, 0);
                // A company layer starts from its builtin, then explicit leaves win.
                let primary = value.pointer("/company/color").and_then(Value::as_str);
                let base = value.pointer("/company/base").and_then(Value::as_str);
                if let Some(base) = base
                    && let Some(base) = builtin_document(base)
                {
                    merge_layer(&mut merged, base);
                }
                if let Some(primary) = primary {
                    let secondary = value.pointer("/company/secondary").and_then(Value::as_str);
                    if let Some(derived) = brand::derive(&merged, primary, secondary) {
                        let adjusted_accent = derived.layer["tokens"]["accent"]
                            .as_str()
                            .unwrap_or(primary)
                            .to_owned();
                        let adjusted_series = derived.layer["chart"]["palette"][0]
                            .as_str()
                            .unwrap_or(primary)
                            .to_owned();
                        let adjusted_link = derived.layer["tokens"]["link"]
                            .as_str()
                            .unwrap_or(primary)
                            .to_owned();
                        // Do not let an earlier layer's accent shortcut overwrite the new company.
                        if let Some(settings) =
                            merged.get_mut("settings").and_then(Value::as_object_mut)
                        {
                            settings.remove("accent");
                        }
                        merge_layer(&mut merged, &derived.layer);
                        if derived.adjusted {
                            diagnostics.push(Diagnostic::new("info", "style.company-adjusted", format!("Company colour {primary} was adjusted to accent {adjusted_accent}, link {adjusted_link} and first series {adjusted_series} to meet contrast on the report surfaces"), format!("{root}.company.color")));
                        }
                        if derived.secondary_skipped {
                            diagnostics.push(Diagnostic::new("info", "style.company-secondary", format!("The secondary colour {} was omitted because it could not be distinguished from the first series {adjusted_series}", secondary.unwrap_or("")), format!("{root}.company.secondary")));
                        } else if let Some(adjusted) = derived.secondary_adjusted {
                            diagnostics.push(Diagnostic::new("info", "style.company-secondary", format!("The secondary colour {} was adjusted to {adjusted} for contrast and distinction", secondary.unwrap_or("")), format!("{root}.company.secondary")));
                        }
                        if derived.palette_failed {
                            diagnostics.push(Diagnostic::new("warning", "style.company-palette", "A distinct six-colour palette could not be derived; the base palette remains", format!("{root}.company.color")));
                        }
                    } else {
                        diagnostics.push(Diagnostic::new("warning", "style.company-palette", "A distinct six-colour palette could not be derived from this brand and base", format!("{root}.company.color")));
                    }
                } else if value.pointer("/company/secondary").is_some() {
                    diagnostics.push(Diagnostic::new(
                        "warning",
                        "style.schema",
                        "A secondary brand colour needs company.color in the same style file",
                        format!("{root}.company.secondary"),
                    ));
                }
                merge_layer(&mut merged, &value);
            }
            Err(err) => {
                let mut d = crate::parser::yaml_diagnostic(&err, &root, "style", 0);
                d.tag = None;
                diagnostics.push(d);
            }
        }
    }
    finalize(&mut merged);
    let mut props = BTreeMap::new();
    for section in ["tokens", "layout"] {
        if let Some(leaves) = merged.get(section).and_then(Value::as_object) {
            for (key, value) in leaves {
                if !INTERNAL_TOKENS.contains(&key.as_str()) {
                    props.insert(format!("--rv-{key}"), text(value));
                }
            }
        }
    }
    refuse_unsafe(&mut props, diagnostics);
    let scheme = merged
        .pointer("/tokens/scheme")
        .and_then(Value::as_str)
        .unwrap_or("light")
        .to_owned();
    // `still` is the character that does not move: the same as `motion: off`.
    if merged["motion"]["character"] == json!("still") {
        merged["motion"]["enabled"] = json!(false);
    }
    let mut css = token_css(&props);
    css.push_str(&format!(":root{{color-scheme:{scheme}}}\n"));
    if let Some(edge) = readout_edge(&props) {
        css.push_str(&format!(":root{{--rv-readout-rule:var({edge})}}\n"));
    }
    css.push_str(&motion_css(&merged["motion"]));
    // Italic is requested only from a family that embeds a true italic face; slanting a
    // roman face is not italic type. Quotes use it wherever it exists; an italic deck is the
    // style's choice (`components.deck.style`).
    if has_italic(
        props
            .get("--rv-font-text")
            .map_or("Newsreader", String::as_str),
    ) {
        css.push_str(".rv-report__body blockquote{font-style:italic}\n");
        if merged
            .pointer("/components/deck/style")
            .and_then(Value::as_str)
            == Some("italic")
        {
            css.push_str(".rv-report__lede,.rv-report__subtitle{font-style:italic}\n");
        }
    }
    // Bold is likewise requested only from a family that embeds it: a display family with one
    // light weight (a lettering face) sets headings, figure titles and stats at that weight.
    if lacks_bold(props.get("--rv-font-display").map_or("", String::as_str)) {
        css.push_str(".rv-report{font-synthesis-weight:none}\n");
    }
    css.push_str(&components::css(merged.get("components"), &props));
    css.push_str(REDUCED_MOTION_CSS);
    if font_catalog().is_empty() {
        diagnostics.push(Diagnostic::new(
            "warning",
            "style.fonts-unavailable",
            "This host did not provide the bundled font catalog; report text falls back to system fonts",
            "fonts",
        ));
    }
    let font_css = fonts_for_style(&props, &merged);
    shift_style_paths(&mut diagnostics[first_diagnostic..], offset);
    ResolvedStyle {
        scheme,
        chart: merged.get("chart").cloned().unwrap_or(json!({})),
        document: merged,
        custom_properties: props,
        css,
        font_css,
    }
}

/// The bundled font catalogs in embedding order. Native builds compile them in; WebAssembly
/// builds leave them out, so the runtime every interactive report embeds stays small.
#[cfg(not(target_arch = "wasm32"))]
const BUNDLED_FONT_CATALOGS: [&str; 5] = [
    include_str!("../assets/fonts-editorial.css"),
    include_str!("../assets/fonts-brutalist.css"),
    include_str!("../assets/fonts-scientific.css"),
    include_str!("../assets/fonts-sleek.css"),
    include_str!("../assets/fonts-hand-drawn.css"),
];

/// The supplied math face, selected after Markdown has produced actual MathML.
pub(crate) fn math_font_css() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        catalog_faces(include_str!("../../../assets/fonts-math.css"))
            .into_iter()
            .map(|(_, face)| format!("{face}\n"))
            .collect()
    }
    #[cfg(target_arch = "wasm32")]
    {
        // A host may supply the math catalog for a self-contained render. The site otherwise
        // links the separate stylesheet only when MathML renders, keeping its first visit small.
        font_catalog()
            .iter()
            .filter(|(family, _)| family == "stix two math")
            .map(|(_, face)| format!("{face}\n"))
            .collect()
    }
}

/// The bundled catalogs as one stylesheet. The build stages it beside each WebAssembly runtime
/// as `richview-fonts.css` for the hosts that render reports there.
#[cfg(not(target_arch = "wasm32"))]
pub fn bundled_font_catalog() -> String {
    BUNDLED_FONT_CATALOGS.concat()
}

#[cfg(target_arch = "wasm32")]
static PROVIDED_FONT_CATALOG: OnceLock<Vec<(String, &'static str)>> = OnceLock::new();

/// Give a WebAssembly build the staged font catalog (`richview-fonts.css`), as hosts give it
/// resolved data. Call it once before rendering; text without `@font-face` rules and later
/// calls are ignored. Returns whether the catalog was accepted.
#[cfg(target_arch = "wasm32")]
pub fn provide_font_catalog(css: String) -> bool {
    if PROVIDED_FONT_CATALOG.get().is_some() || !css.contains("@font-face") {
        return false;
    }
    // One catalog lives as long as the module, like the compiled-in catalogs of native builds.
    let css: &'static str = Box::leak(css.into_boxed_str());
    PROVIDED_FONT_CATALOG.set(catalog_faces(css)).is_ok()
}

/// Every bundled `@font-face` rule with its lowercase family name.
fn font_catalog() -> &'static [(String, &'static str)] {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static CATALOG: OnceLock<Vec<(String, &'static str)>> = OnceLock::new();
        CATALOG.get_or_init(|| {
            BUNDLED_FONT_CATALOGS
                .into_iter()
                .flat_map(catalog_faces)
                .collect()
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        PROVIDED_FONT_CATALOG.get().map_or(&[], Vec::as_slice)
    }
}

fn catalog_faces(catalog: &'static str) -> Vec<(String, &'static str)> {
    catalog
        .lines()
        .filter(|line| line.starts_with("@font-face"))
        .filter_map(|face| {
            let family = face
                .split_once("font-family: \"")
                .and_then(|(_, tail)| tail.split_once('"'))
                .map(|(family, _)| family.to_lowercase());
            family.map(|family| (family, face))
        })
        .collect()
}

/// Whether the first family of a font stack embeds a true italic face.
fn has_italic(stack: &str) -> bool {
    let family = first_family(stack);
    font_catalog().iter().any(|(name, face)| {
        *name == family
            && face
                .split_once("font-style:")
                .is_some_and(|(_, style)| style.trim_start().starts_with("italic"))
    })
}

/// Whether the first family of a font stack is bundled with no face of weight 600 or more.
fn lacks_bold(stack: &str) -> bool {
    let family = first_family(stack);
    let mut weights = font_catalog()
        .iter()
        .filter(|(name, _)| *name == family)
        .filter_map(|(_, face)| {
            face.split_once("font-weight:")
                .and_then(|(_, rest)| rest.trim_start().split(';').next())
                .and_then(|weight| weight.trim().parse::<u32>().ok())
        })
        .peekable();
    weights.peek().is_some() && weights.all(|weight| weight < 600)
}

fn first_family(stack: &str) -> String {
    stack
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(['\'', '"'])
        .to_lowercase()
}

fn fonts_for_style(props: &BTreeMap<String, String>, doc: &Value) -> String {
    let mut families = std::collections::BTreeSet::new();
    for name in [
        "--rv-font-text",
        "--rv-font-display",
        "--rv-font-ui",
        "--rv-font-mono",
    ] {
        let fallback = if name == "--rv-font-text" {
            "Newsreader"
        } else if name == "--rv-font-mono" {
            "RichView Mono"
        } else {
            ""
        };
        let value = props.get(name).map(String::as_str).unwrap_or(fallback);
        for family in value.split(',') {
            families.insert(family.trim().trim_matches(['\'', '"']).to_lowercase());
        }
    }
    for name in ["font-family", "font-numeric"] {
        if let Some(value) = doc
            .get("chart")
            .and_then(|chart| chart.get(name))
            .and_then(Value::as_str)
        {
            for family in value.split(',') {
                families.insert(family.trim().trim_matches(['\'', '"']).to_lowercase());
            }
        }
    }
    let mut css = String::new();

    for (family, face) in font_catalog() {
        if families.contains(family) {
            css.push_str(face);
            css.push('\n');
        }
    }
    css
}

/// Scale a length written in px or rem; other units and numbers keep their value.
fn scale_length(value: &str, factor: f64) -> String {
    for unit in ["px", "rem"] {
        if let Some(n) = value.strip_suffix(unit).and_then(|n| n.parse::<f64>().ok()) {
            let scaled = format!("{:.3}", n * factor);
            return format!(
                "{}{unit}",
                scaled.trim_end_matches('0').trim_end_matches('.')
            );
        }
    }
    value.to_owned()
}

/// Apply the settings that depend on the whole cascade (STY-15): `type-size` scales the type
/// scale as one, and `accent` sets the accent, its soft fill, the links where the accent reads
/// as text, and the first series where that is the style's accent.
fn finalize(merged: &mut Value) {
    use richview_chart::color;
    let settings = merged.get("settings").cloned().unwrap_or(json!({}));
    if let Some(factor) = settings.get("type-scale").and_then(Value::as_f64)
        && factor != 1.0
        && let Some(tokens) = merged.get_mut("tokens").and_then(Value::as_object_mut)
    {
        for (key, value) in tokens.iter_mut() {
            if size_token(key)
                && let Some(text) = value.as_str()
            {
                *value = json!(scale_length(text, factor));
            }
        }
    }
    let Some(accent) = settings.get("accent").and_then(Value::as_str) else {
        return;
    };
    let token = |merged: &Value, name: &str| {
        merged
            .pointer(&format!("/tokens/{name}"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let old = token(merged, "accent").and_then(|c| color::parse(&c));
    let new = color::parse(accent);
    let surfaces: Vec<_> = ["bg", "bg-elev"]
        .into_iter()
        .filter_map(|name| token(merged, name).and_then(|c| color::parse(&c)))
        .collect();
    if !merged["tokens"].is_object() {
        merged["tokens"] = json!({});
    }
    let tokens = merged["tokens"]
        .as_object_mut()
        .expect("tokens is a mapping");
    tokens.insert("accent".into(), json!(accent));
    let Some(new) = new else {
        return;
    };
    tokens.insert(
        "accent-soft".into(),
        json!(format!(
            "oklch({:.4} {:.4} {:.2} / 0.12)",
            new.l, new.c, new.h
        )),
    );
    // Links keep the style's own colour where the accent is too weak to read as text.
    if surfaces
        .iter()
        .all(|surface| color::contrast(new, *surface) >= 4.5)
    {
        tokens.insert("link".into(), json!(accent));
    }
    // The first series follows the accent when it was the style's accent.
    let chart = merged["chart"].as_object_mut();
    let Some(chart) = chart else {
        return;
    };
    let mut palette: Vec<String> = match chart.get("palette") {
        Some(Value::String(name)) => richview_chart::rmd_render::palette(name)
            .iter()
            .map(|c| (*c).to_owned())
            .collect(),
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => return,
    };
    let first = palette.first().and_then(|c| color::parse(c));
    if let (Some(first), Some(old)) = (first, old)
        && first.hex() == old.hex()
    {
        palette[0] = accent.to_owned();
        chart.insert("palette".into(), json!(palette));
    }
}

/// Merges one style layer. A layer that names a `motion.character` and leaves some timing
/// leaves out takes those from the character's preset, over whatever lower layers set, so
/// `motion: {character: crisp}` alone is a crisp style (INT-12).
fn merge_layer(merged: &mut Value, layer: &Value) {
    let preset = layer["motion"]["character"]
        .as_str()
        .and_then(motion_preset);
    let Some(preset) = preset else {
        return merge_into(merged, layer);
    };
    let mut layer = layer.clone();
    for (key, value) in preset {
        if layer["motion"].get(key).is_none() {
            layer["motion"][key] = value;
        }
    }
    merge_into(merged, &layer);
}

/// The timing a motion character stands for; the builtins set the same values in their files.
fn motion_preset(character: &str) -> Option<[(&'static str, Value); 5]> {
    let (duration, easing, stagger, entrance, distance) = match character {
        "restrained" => (240, "standard", 16, "fade", 0),
        "gentle" => (560, "out", 40, "fade-up", 14),
        "playful" => (720, "spring", 70, "fade-up", 16),
        "crisp" => (160, "snap", 0, "none", 0),
        _ => return None,
    };
    Some([
        ("duration", json!(duration)),
        ("easing", json!(easing)),
        ("stagger", json!(stagger)),
        ("block-entrance", json!(entrance)),
        ("distance", json!(distance)),
    ])
}

/// Later files win per leaf and lists replace (TYP-44).
fn merge_into(merged: &mut Value, value: &Value) {
    match (merged, value) {
        (Value::Object(into), Value::Object(from)) => {
            for (key, val) in from {
                match into.get_mut(key) {
                    Some(existing) if existing.is_object() && val.is_object() => {
                        merge_into(existing, val);
                    }
                    _ => {
                        into.insert(key.clone(), val.clone());
                    }
                }
            }
        }
        (merged, value) => *merged = value.clone(),
    }
}

fn regex<'a>(cell: &'a OnceLock<Regex>, pattern: &str) -> &'a Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("static grammar"))
}

/// A colour of the style grammar: hex of 3, 4, 6 or 8 digits, a colour function, one of the 148
/// CSS Color 4 names, `transparent` or `currentcolor`, in any case. The chart engine reads every
/// form but the last two (`richview_chart::color::parse`), which no chart colour may use.
pub(crate) fn valid_color(value: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    value.len() <= 80
        && (regex(
            &RE,
            r"(?i)^(?:(?:oklch|lch|lab|oklab|hsla?|rgba?)\([-+0-9.,%/ e]+\)|#(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8}))$",
        )
        .is_match(value)
            || richview_chart::color::named(value).is_some()
            || ["transparent", "currentcolor"].contains(&value.to_ascii_lowercase().as_str()))
}

pub(crate) fn valid_font(value: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    !value.is_empty()
        && value.len() <= 200
        && regex(&RE, r#"^[a-zA-Z0-9_ .,'"+\-]+$"#).is_match(value)
        && quotes_close(value)
}

/// Every quote opened in a font stack closes; the other kind inside it is plain text. An
/// unclosed quote would swallow the rest of its declaration.
fn quotes_close(value: &str) -> bool {
    let mut open = None;
    for c in value.chars().filter(|c| matches!(c, '\'' | '"')) {
        match open {
            None => open = Some(c),
            Some(q) if q == c => open = None,
            Some(_) => {}
        }
    }
    open.is_none()
}
fn valid_dim(value: &Value) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    value.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0)
        || value.as_str().is_some_and(|s| {
            s.len() <= 20
                && regex(
                    &RE,
                    r"^[0-9]+(?:\.[0-9]+)?(?:px|ch|rem|em|%|vh|vw|fr|s|ms)?$",
                )
                .is_match(s)
        })
}
/// A shadow: `none`, or offsets, blur and one colour in any form [`valid_color`] takes, as in
/// `0 2px 8px oklch(0 0 0 / 0.12)`. Authors write every other colour in any form (STY-19,
/// TYP-46), so a shadow does too (S27-931).
fn valid_shadow(value: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    value == "none"
        || regex(
            &RE,
            r"^-?[0-9]+(?:\.[0-9]+)?(?:px)? -?[0-9]+(?:\.[0-9]+)?(?:px)? [0-9]+(?:\.[0-9]+)?px (\S.*)$",
        )
        .captures(value)
        .and_then(|parts| parts.get(1))
        .is_some_and(|colour| valid_color(colour.as_str()))
}

/// The type scale: body, small and heading sizes, each with an optional phone value.
fn size_token(key: &str) -> bool {
    matches!(
        key.strip_suffix(PHONE_SUFFIX).unwrap_or(key),
        "size-base" | "size-small" | "size-h3" | "size-h2" | "size-h1" | "size-display"
    )
}

fn valid_letter_spacing(value: &Value) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    value.as_f64().is_some_and(|n| n.is_finite())
        || value.as_str().is_some_and(|s| {
            s.len() <= 20
                && (s == "normal"
                    || regex(
                        &RE,
                        r"^-?(?:[0-9]+(?:\.[0-9]+)?|\.[0-9]+)(?:px|ch|rem|em)?$",
                    )
                    .is_match(s))
        })
}

/// Palette names `richview_chart::rmd_render::palette` knows: one per builtin (TYP-52).
const PALETTE_NAMES: [&str; 5] = ["sleek", "air", "scientific", "brutalist", "hand-drawn"];

/// A colour the chart engine can draw: `transparent` and `currentcolor` are valid style colours
/// but resolve to no colour of its own, and a series that silently lost its colour would draw
/// with another's.
fn chart_color(value: &str) -> bool {
    valid_color(value) && richview_chart::color::parse(value).is_some()
}

fn color_list(value: &Value, len: std::ops::RangeInclusive<usize>) -> bool {
    value.as_array().is_some_and(|items| {
        len.contains(&items.len()) && items.iter().all(|v| v.as_str().is_some_and(chart_color))
    })
}

/// The light builtin a dark builtin prints as: paper and PDF pages are always light.
fn print_counterpart(name: &str) -> Option<&'static str> {
    let doc = builtin_document(name)?;
    let counterpart = doc.pointer("/tokens/print-as")?.as_str()?;
    builtin_style_names()
        .iter()
        .copied()
        .find(|builtin| *builtin == counterpart && *builtin != name)
}

/// The cascade of a printed page: each dark builtin replaced by its light counterpart.
pub(crate) fn print_refs(refs: &[String]) -> Vec<String> {
    refs.iter()
        .map(|reference| {
            if let Some(name) = print_counterpart(reference) {
                return name.to_owned();
            }
            // A brand's base lives inside a style source, not as a cascade reference. Re-derive
            // its colours over that builtin's print counterpart instead of dropping the company.
            let Ok(mut source) = crate::yaml::from_str(reference) else {
                return reference.clone();
            };
            let Some(base) = source
                .pointer("/company/base")
                .and_then(Value::as_str)
                .and_then(print_counterpart)
            else {
                return reference.clone();
            };
            source["company"]["base"] = json!(base);
            serde_json::to_string(&source).unwrap_or_else(|_| reference.clone())
        })
        .collect()
}

/// Surfaces, inks, semantic colours and chart colours a dark cascade takes from Air when it prints (STY-21).
const PAPER_TOKENS: [&str; 16] = [
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
];

/// The cascade of the printed page, when it differs from the screen's: each dark builtin
/// replaced by its light counterpart, and a cascade still dark after that (an author's style
/// file darkened a light builtin) taking Air's surfaces, inks and chart colours. The flag is set
/// for that last case, which warns `style.print-dark`.
pub(crate) fn print_cascade(
    refs: &[String],
    screen: &ResolvedStyle,
) -> (Option<ResolvedStyle>, bool) {
    let light_refs = print_refs(refs);
    // Resolving the screen cascade has already reported these styles' diagnostics.
    let mut paper = (light_refs != refs).then(|| resolve_styles(&light_refs, &mut Vec::new()));
    let current = paper.as_ref().unwrap_or(screen);
    // An author darkens a light builtin through its colours, so the page colour decides.
    let dark = current.scheme == "dark"
        || current.custom_properties.get("--rv-bg").is_some_and(|bg| {
            richview_chart::color::parse(bg).is_some_and(|colour| colour.l < 0.5)
        });
    if !dark {
        return (paper, false);
    }
    let air = resolve_styles(&["air".to_owned()], &mut Vec::new());
    let mut printed = current.clone();
    for name in PAPER_TOKENS {
        if let Some(value) = air.document["tokens"].get(name) {
            printed.document["tokens"][name] = value.clone();
        }
        if let Some(value) = air.custom_properties.get(&format!("--rv-{name}")) {
            printed
                .custom_properties
                .insert(format!("--rv-{name}"), value.clone());
        }
    }
    for key in ["palette", "ramp", "negative-ramp"] {
        if let Some(value) = air.chart.get(key) {
            printed.chart[key] = value.clone();
        }
    }
    printed.scheme = "light".into();
    paper = Some(printed);
    (paper, true)
}

/// The files the style's brand fonts name (STY-16), for a host to read into
/// `RenderOptions::data` as data URIs keyed by these paths.
pub fn brand_font_files(style: &ResolvedStyle) -> Vec<String> {
    style.document["settings"]["brand-fonts"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(_, setting)| setting["file"].as_str().map(str::to_owned))
        .collect()
}

/// Embeds each brand font whose file the host supplied, and warns `style.font-missing` for
/// each whose file is absent: a declared family is no embedded face until its file arrives.
pub(crate) fn embed_brand_fonts(
    style: &mut ResolvedStyle,
    data: &BTreeMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(fonts) = style.document["settings"]["brand-fonts"]
        .as_object()
        .cloned()
    else {
        return;
    };
    let mut embedded: Vec<(String, String, u64)> = vec![];
    for (role, setting) in fonts {
        let (Some(family), Some(file)) = (setting["family"].as_str(), setting["file"].as_str())
        else {
            continue;
        };
        // Only a WOFF2 payload is embedded: the exact media type, the base64 alphabet, the
        // `wOF2` signature and a size cap. Anything else is refused (INT-28, ARC-12).
        let problem = match data.get(file) {
            None => Some(
                "is not in the report folder; the report falls back to the next family of its stack"
                    .to_owned(),
            ),
            Some(uri) if !is_woff2_data_uri(uri) => Some(
                "is not a WOFF2 file of at most 3 MiB encoded; the report falls back to the next family of its stack"
                    .to_owned(),
            ),
            Some(_) if !family.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_')) => Some(
                "has a family name with characters other than letters, digits, spaces, - and _".to_owned(),
            ),
            Some(_) => None,
        };
        if let Some(problem) = problem {
            diagnostics.push(Diagnostic::new(
                "warning",
                "style.font-missing",
                format!("Brand font file '{file}' for '{family}' {problem}"),
                format!("tokens.font-{role}"),
            ));
            continue;
        }
        let source = &data[file];
        let weight = setting["weight"].as_u64().unwrap_or(400);
        // A family and file shared by two roles are embedded once.
        let key = (family.to_owned(), file.to_owned(), weight);
        if embedded.contains(&key) {
            continue;
        }
        embedded.push(key);
        style.font_css.push_str(&format!(
            "/* brand font */ @font-face {{ font-family: \"{family}\"; font-style: normal; font-weight: {weight}; font-display: block; src: url(\"{source}\") format(\"woff2\"); }}\n"
        ));
    }
}

/// Paper settings of a style cascade.
pub(crate) struct PrintStyle {
    /// `@media print` rules: the light cascade's tokens, with the type scale and measure at
    /// paper size, and the page background.
    pub css: String,
    /// Literal colours and face for `@page` margin boxes, where custom properties do not apply.
    pub bg: String,
    pub ink_soft: String,
    pub font_ui: String,
}

/// Paper sets type at 80% of the screen scale, so 18 px body text prints at 10.8 pt.
const PRINT_TYPE_SCALE: f64 = 0.8;

/// Print tokens for `refs`, whose screen cascade is `screen`. The tokens are declared for
/// `:root`; the page prints light whatever the screen's scheme (STY-21).
pub(crate) fn print_style(refs: &[String], screen: &ResolvedStyle) -> PrintStyle {
    let (light, _) = print_cascade(refs, screen);
    // Paper takes the light style's colours and sizes but keeps the faces the file embeds.
    let mut props = light.as_ref().unwrap_or(screen).custom_properties.clone();
    for (key, value) in &screen.custom_properties {
        if key.starts_with("--rv-font-") {
            props.insert(key.clone(), value.clone());
        }
    }
    let token = |name: &str| props.get(name).cloned();
    // Charts print from the light theme their payload carries (FEA-CH-67).
    let declarations = props
        .iter()
        .filter(|(key, _)| !key.ends_with(PHONE_SUFFIX))
        .map(|(key, value)| {
            let name = key.trim_start_matches("--rv-");
            let value = if size_token(name) || name == "measure" {
                print_length(value)
            } else {
                value.clone()
            };
            format!("{key}:{value};")
        })
        .collect::<String>();
    let bg = token("--rv-bg").unwrap_or_else(|| "oklch(1 0 0)".into());
    PrintStyle {
        css: format!("@media print{{@page{{background:{bg}}}:root{{{declarations}}}}}\n"),
        bg,
        ink_soft: token("--rv-ink-soft").unwrap_or_else(|| "oklch(0.4495 0 0)".into()),
        font_ui: token("--rv-font-ui")
            .or_else(|| token("--rv-font-text"))
            .unwrap_or_else(|| "sans-serif".into()),
    }
}

/// An absolute length at paper size. Font-relative lengths (`em`, `ch`) already follow the
/// smaller type.
fn print_length(value: &str) -> String {
    for unit in ["px", "rem"] {
        if let Some(n) = value.strip_suffix(unit).and_then(|n| n.parse::<f64>().ok()) {
            let scaled = format!("{:.2}", n * PRINT_TYPE_SCALE);
            return format!(
                "{}{unit}",
                scaled.trim_end_matches('0').trim_end_matches('.')
            );
        }
    }
    value.into()
}

/// Report text, stats and callouts are static. Chart motion runs in the Rust runtime, which
/// also honours this preference; the rule stops any remaining CSS animation or transition.
const REDUCED_MOTION_CSS: &str = "@media(prefers-reduced-motion:reduce){*,*::before,*::after{animation:none!important;transition:none!important;scroll-behavior:auto!important}}\n";

/// Declarative CSS is shipped separately from the engine and never interpreted as code.
pub(crate) fn base_css() -> &'static str {
    include_str!("../assets/report.css")
}

/// The style's motion as custom properties the renderer's block entrances read (INT-12). They
/// describe the settled-to-arrived transition only: reduced motion and a page without script
/// always render settled, whatever these say.
fn motion_css(motion: &Value) -> String {
    if !motion.is_object() {
        return String::new();
    }
    let [a, b, c, d] =
        richview_chart::motion::easing(motion["easing"].as_str().unwrap_or("standard"));
    // A quoted number passes the grammar, so it reads like a bare one.
    let number = |key: &str, default: f64| {
        motion[key]
            .as_f64()
            .or_else(|| motion[key].as_str().and_then(|text| text.parse().ok()))
            .unwrap_or(default)
    };
    let enabled = motion["enabled"].as_bool().unwrap_or(true);
    let entrance = if enabled {
        motion["block-entrance"].as_str().unwrap_or("fade-up")
    } else {
        "none"
    };
    format!(
        ":root{{--rv-motion-duration:{}ms;--rv-motion-easing:cubic-bezier({a},{b},{c},{d});--rv-motion-stagger:{}ms;--rv-motion-distance:{}px;--rv-motion-block:{entrance}}}\n",
        number("duration", 500.0),
        number("stagger", 40.0),
        // A fade stays where it is; only a fade-up rises.
        if entrance == "fade" {
            0.0
        } else {
            number("distance", 12.0)
        },
    )
}

/// A `data:font/woff2;base64,` URI whose payload is base64 of at most 3 MiB and starts with
/// the four bytes `wOF2`. The flavour bytes after the signature (TrueType, CFF) are free.
fn is_woff2_data_uri(uri: &str) -> bool {
    const MAX_FONT_BASE64: usize = 3 * 1024 * 1024;
    let Some(payload) = uri.strip_prefix("data:font/woff2;base64,") else {
        return false;
    };
    let value = |byte: u8| -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some(u32::from(byte - b'A')),
            b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let bytes = payload.as_bytes();
    if bytes.len() > MAX_FONT_BASE64 || bytes.len() < 8 {
        return false;
    }
    if !bytes.iter().all(|b| value(*b).is_some() || *b == b'=') {
        return false;
    }
    // The first 32 bits are the signature: 5 full characters and the top 2 bits of the 6th.
    let bits: u32 = bytes[..5]
        .iter()
        .fold(0, |acc, b| (acc << 6) | value(*b).unwrap_or(0));
    let sixth = value(bytes[5]).unwrap_or(0);
    let first32 = (bits << 2) | (sixth >> 4);
    first32 == u32::from_be_bytes(*b"wOF2")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dark_company_rederives_its_palette_for_print() {
        let source = "company:\n  color: '#0A1F44'\n  base: sleek\n".to_owned();
        let refs = vec![source];
        let screen = resolve_styles(&refs, &mut vec![]);
        let (paper, fallback) = print_cascade(&refs, &screen);
        assert!(!fallback, "a company colour derives over the print base");
        let paper = paper.expect("Sleek has a light print counterpart");
        assert_eq!(paper.scheme, "light");
        assert!(
            checks::contrast(&paper).is_empty(),
            "{:?}",
            checks::contrast(&paper)
        );
        assert!(
            checks::palette(&paper).is_empty(),
            "{:?}",
            checks::palette(&paper)
        );
        assert!(
            checks::ramps(&paper).is_empty(),
            "{:?}",
            checks::ramps(&paper)
        );
        assert_ne!(
            paper.chart["palette"],
            resolve_styles(&["air".into()], &mut vec![]).chart["palette"]
        );
    }

    #[test]
    fn sinks_refuse_what_no_grammar_admits() {
        for bad in [
            "a;b", "a{b", "a}", "<b>", "a\\b", "@import", "URL(x)", "url(x)",
        ] {
            assert!(sink_refuses(bad), "{bad}");
        }
        for fine in [
            "oklch(0.5 0.1 30 / 0.5)",
            "'Geist', system-ui",
            "0 2px 8px oklch(0 0 0 / 0.12)",
        ] {
            assert!(!sink_refuses(fine), "{fine}");
        }
        let mut props = BTreeMap::from([
            ("--rv-bg".to_owned(), "white".to_owned()),
            ("--rv-evil".to_owned(), "red;}".to_owned()),
        ]);
        let mut diagnostics = vec![];
        refuse_unsafe(&mut props, &mut diagnostics);
        assert_eq!(props.len(), 1);
        assert_eq!(diagnostics[0].code, "style.sink");
        assert_eq!(diagnostics[0].path, "--rv-evil");
    }

    #[test]
    fn shadows_take_every_colour_form_the_style_grammar_takes() {
        for fine in [
            "none",
            "0 2px 8px oklch(0 0 0 / 0.12)",
            "0 2px 8px #0000001f",
            "0 2px 8px #000",
            "-1 2.5px 0px rgb(0 0 0 / 0.2)",
            "0 1px 2px hsl(220 20% 10% / 0.3)",
            "0 2px 8px Black",
            "0 2px 8px transparent",
        ] {
            assert!(valid_shadow(fine), "{fine}");
        }
        for bad in [
            "",
            "0 2px oklch(0 0 0)",
            "0 2px 8px",
            "0 2px 8px ",
            "0 2px 8 #000",
            "0 2px 8px #00000",
            "0 2px 8px notacolour",
            "0 2px 8px #000, 0 0 1px #000",
            "0 2px 8px #000;}",
            "0 2px 8px url(x)",
        ] {
            assert!(!valid_shadow(bad), "{bad:?}");
        }
    }
}
