//! Resolve an authored company's validated logo from host-supplied report resources.
use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::STANDARD};

use crate::{CompanyLogoError, Diagnostic, RenderOptions, ResolvedStyle, report_relative_path};

/// A static, bounded raster suitable for embedding in a report or its share card.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanyLogo {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
}

impl CompanyLogo {
    /// Encode the validated bytes with their sniffed media type, never the file extension's type.
    pub fn data_uri(&self) -> String {
        format!("data:{};base64,{}", self.mime, STANDARD.encode(&self.bytes))
    }
}

/// The report's company style and its light- and dark-surface marks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompanyBrand {
    pub name: String,
    /// The page surface on which the resolved mark will appear.
    pub dark_surface: bool,
    /// The chosen builtin underneath this company style, when explicitly set.
    pub base: Option<String>,
    /// The resolved company accent in sRGB, for outputs that cannot use CSS colour syntax.
    pub accent: Option<String>,
    pub logo: Option<CompanyLogo>,
    pub logo_dark: Option<CompanyLogo>,
}

impl CompanyBrand {
    /// Render the validated mark before the report title, with the company name as its alt text.
    pub(crate) fn header_html(&self, dark: bool) -> String {
        let Some(logo) = &self.logo else {
            return String::new();
        };
        let mut class = String::from("rv-report__logo");
        if dark {
            class.push_str(" rv-report__logo--dark");
            if self.logo_dark.is_none() {
                class.push_str(" rv-report__logo--plate");
            }
        }
        if self.logo_dark.is_some() {
            class.push_str(" rv-report__logo--has-dark");
        }
        let alt = crate::escape(&self.name);
        let mut html = format!(
            "<p class=\"{class}\"><img class=\"rv-report__logo-light\" src=\"{}\" alt=\"{alt}\" width=\"{}\" height=\"{}\">",
            logo.data_uri(),
            logo.width,
            logo.height
        );
        if let Some(dark_logo) = &self.logo_dark {
            html.push_str(&format!(
                "<img class=\"rv-report__logo-dark\" src=\"{}\" alt=\"{alt}\" width=\"{}\" height=\"{}\">",
                dark_logo.data_uri(), dark_logo.width, dark_logo.height
            ));
        }
        html.push_str("</p>");
        html
    }

    /// Size each validated raster from its own aspect ratio, including the print mark.
    pub(crate) fn header_css(&self) -> String {
        let mut css = String::new();
        for (logo, class) in [
            (self.logo.as_ref(), "rv-report__logo-light"),
            (self.logo_dark.as_ref(), "rv-report__logo-dark"),
        ] {
            let Some(logo) = logo else { continue };
            let ratio = f64::from(logo.width) / f64::from(logo.height);
            let mut height = (4.5 / ratio.sqrt()).clamp(1.25, 2.5);
            let mut width = ratio * height;
            if width > 12.0 {
                width = 12.0;
                height = width / ratio;
            }
            css.push_str(&format!(
                ".rv-report__logo .{class}{{width:{width:.3}rem;height:{height:.3}rem}}@media(max-width:640px){{.rv-report__logo .{class}{{width:{:.3}rem;height:{:.3}rem}}}}",
                width * 0.9, height * 0.9
            ));
        }
        // A light logo on a dark style stands on its own white plate, on screen only: paper prints
        // the light style, where the logo needs none. The base stylesheet names no colour.
        if self.logo.is_some() && self.logo_dark.is_none() {
            css.push_str("@media screen{.rv-report__logo--plate{background:oklch(1 0 0)}}");
        }
        css
    }
}

/// Validate the company marks supplied by a host without performing any file access.
pub fn resolve_company_brand(
    style: &ResolvedStyle,
    data: &BTreeMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> CompanyBrand {
    let company = &style.document["company"];
    let name = company["name"].as_str().unwrap_or("").trim().to_owned();
    let logo = read_logo(company["logo"].as_str(), "logo", data, diagnostics);
    let logo_dark = read_logo(
        company["logo-dark"].as_str(),
        "logo-dark",
        data,
        diagnostics,
    );
    if company["logo-dark"].is_string() && !company["logo"].is_string() {
        diagnostics.push(
            Diagnostic::new(
                "error",
                "style.logo-missing",
                "A dark company logo needs a standard logo for light and printed pages",
                "company.logo",
            )
            .fix("Add company.logo or remove company.logo-dark"),
        );
    }
    if name.is_empty() && (company["logo"].is_string() || company["logo-dark"].is_string()) {
        diagnostics.push(
            Diagnostic::new(
                "warning",
                "style.logo-name",
                "The company logo has no company name, so it is decorative",
                "company.name",
            )
            .fix("Set company.name to the company name, or leave the mark decorative"),
        );
    }
    // The new syntax names the preset and the brand colour outside `company` (STY-73).
    let branded = company.is_object() || style.document.pointer("/settings/colors/brand").is_some();
    CompanyBrand {
        name,
        dark_surface: style.scheme == "dark",
        base: style
            .document
            .pointer("/settings/preset")
            .and_then(|p| p.as_str())
            .map(str::to_owned),
        accent: branded.then_some(()).and_then(|_| {
            style.document["tokens"]["accent"]
                .as_str()
                .and_then(richview_chart::color::parse)
                .map(richview_chart::color::Oklch::hex)
        }),
        logo,
        logo_dark,
    }
}

/// A report's style sources in the renderer's order, and the style they resolve to: the
/// frontmatter's styles, a file as the text the host supplied, then the call's own styles.
fn report_style(
    frontmatter: &serde_json::Value,
    options: &RenderOptions,
) -> (Vec<String>, ResolvedStyle) {
    // The same bound the render keeps (`style_list_problem`): a report that is stored passed it, one
    // stored before it was made is read as far as it allows and no further.
    let mut sources: Vec<String> = Vec::new();
    let mut expanded = 0usize;
    for reference in frontmatter["styles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .take(crate::MAX_STYLE_REFERENCES)
    {
        if crate::is_style_name(reference) {
            sources.push(reference.to_owned());
        } else if let Some(text) = options.data.get(reference) {
            expanded = expanded.saturating_add(text.len());
            if expanded > crate::MAX_STYLE_BYTES {
                break;
            }
            sources.push(text.clone());
        }
    }
    sources.extend(options.styles.iter().cloned());
    sources.extend(crate::report_background_layer(frontmatter));
    let style = crate::resolve_styles(&sources, &mut Vec::new());
    (sources, style)
}

/// The logo files a report's styles name (STY-72), for a host to read into
/// `RenderOptions::data` as data URIs keyed by these paths. A style file names them, so a host
/// that is sent files one by one asks again once its style files are in `options.data`: every
/// surface then renders the same header (ARC-08).
pub fn report_company_logo_files(
    frontmatter: &serde_json::Value,
    options: &RenderOptions,
) -> Vec<String> {
    let (_, style) = report_style(frontmatter, options);
    ["logo", "logo-dark"]
        .into_iter()
        .filter_map(|key| style.document["company"][key].as_str().map(str::to_owned))
        .collect()
}

/// Resolve a report's company mark using the same style order as the renderer.
pub fn report_company_brand(
    frontmatter: &serde_json::Value,
    options: &RenderOptions,
) -> CompanyBrand {
    let (sources, style) = report_style(frontmatter, options);
    let mut company = resolve_company_brand(&style, &options.data, &mut Vec::new());
    // A colour-only company file takes Air when the author did not select a builtin (STY-71).
    if company.base.is_none()
        && style.document.pointer("/settings/colors/brand").is_some()
        && !sources
            .iter()
            .any(|source| crate::builtin_style_names().contains(&source.as_str()))
    {
        company.base = Some("air".into());
    }
    company
}

/// What a raster read from the report folder is for: its name in messages, its diagnostic codes
/// and its bounds.
struct Raster {
    what: &'static str,
    code: &'static str,
    max_bytes: usize,
    max_dimension: u32,
}

const LOGO: Raster = Raster {
    what: "Company logo",
    code: "style.logo",
    max_bytes: crate::COMPANY_LOGO_MAX_BYTES,
    max_dimension: crate::COMPANY_LOGO_MAX_DIMENSION,
};

const PICTURE: Raster = Raster {
    what: "Background picture",
    code: "style.background",
    max_bytes: crate::BACKGROUND_PICTURE_MAX_BYTES,
    max_dimension: crate::BACKGROUND_PICTURE_MAX_DIMENSION,
};

fn read_logo(
    reference: Option<&str>,
    key: &str,
    data: &BTreeMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CompanyLogo> {
    read_raster(
        reference?,
        &format!("company.{key}"),
        &LOGO,
        data,
        diagnostics,
    )
}

/// The report's background picture (STY-74), validated from the bytes a host supplied, with a
/// finding when it is outside the folder, missing, too large or not a still raster.
pub(crate) fn read_background_picture(
    style: &ResolvedStyle,
    data: &BTreeMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CompanyLogo> {
    let reference = style
        .document
        .pointer("/settings/background/picture")
        .and_then(serde_json::Value::as_str)
        .filter(|path| *path != "none")?;
    read_raster(reference, "background.picture", &PICTURE, data, diagnostics)
}

/// The media files a report's styles name, for a host to read into `RenderOptions::data`: the
/// company logos (STY-72) and the background picture (STY-74).
pub fn report_style_media_files(
    frontmatter: &serde_json::Value,
    options: &RenderOptions,
) -> Vec<String> {
    let (_, style) = report_style(frontmatter, options);
    let mut files: Vec<String> = ["logo", "logo-dark"]
        .into_iter()
        .filter_map(|key| style.document["company"][key].as_str().map(str::to_owned))
        .collect();
    if let Some(picture) = style
        .document
        .pointer("/settings/background/picture")
        .and_then(serde_json::Value::as_str)
        .filter(|path| *path != "none")
    {
        files.push(picture.to_owned());
    }
    files
}

fn read_raster(
    reference: &str,
    path: &str,
    kind: &Raster,
    data: &BTreeMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CompanyLogo> {
    let what = kind.what;
    let kib = kind.max_bytes / 1024;
    let max = kind.max_dimension;
    let failure = |diagnostics: &mut Vec<Diagnostic>, suffix: &str, message: String| {
        let logo = kind.code == LOGO.code;
        let fix = match suffix {
            "path" => "Use a file path inside the report folder".to_owned(),
            "missing" if logo => "Put the logo file inside the report folder".to_owned(),
            "size" if logo => "Use a logo no larger than 512 KiB".to_owned(),
            "dimensions" if logo => {
                "Use a logo no larger than 1024 pixels on either side".to_owned()
            }
            "animated" if logo => "Use a still PNG, JPEG or WebP logo".to_owned(),
            "format" if logo => "Use a valid still PNG, JPEG or WebP logo".to_owned(),
            "missing" => format!(
                "Put the {} file inside the report folder",
                what.to_lowercase()
            ),
            "size" => format!("Use a file no larger than {kib} KiB"),
            "dimensions" => format!("Use an image no larger than {max} pixels on either side"),
            "animated" => "Use a still PNG, JPEG or WebP image".to_owned(),
            _ => "Use a valid still PNG, JPEG or WebP image".to_owned(),
        };
        let code = format!("{}-{suffix}", kind.code);
        diagnostics.push(Diagnostic::new("error", &code, message, path).fix(fix));
    };
    if !report_relative_path(reference) {
        failure(
            diagnostics,
            "path",
            format!("The {} must be a report-relative file", what.to_lowercase()),
        );
        return None;
    }
    let Some(uri) = data.get(reference) else {
        failure(
            diagnostics,
            "missing",
            format!("{what} '{reference}' is not available in the report folder"),
        );
        return None;
    };
    let Some((prefix, encoded)) = uri
        .split_once(";base64,")
        .filter(|(prefix, _)| prefix.starts_with("data:"))
    else {
        failure(
            diagnostics,
            "format",
            format!("{what} '{reference}' is not a supplied raster file"),
        );
        return None;
    };
    let _ = prefix;
    // The host retains at most one byte beyond the limit. Direct callers get the same bound.
    if encoded.len() > (kind.max_bytes + 1).div_ceil(3) * 4 {
        failure(
            diagnostics,
            "size",
            format!("{what} '{reference}' exceeds {kib} KiB"),
        );
        return None;
    }
    let Ok(bytes) = STANDARD.decode(encoded) else {
        failure(
            diagnostics,
            "format",
            format!("{what} '{reference}' has invalid encoded bytes"),
        );
        return None;
    };
    match crate::company_logo::validate_raster(&bytes, kind.max_bytes, kind.max_dimension) {
        Ok(image) => Some(CompanyLogo {
            bytes,
            mime: image.mime,
            width: image.width,
            height: image.height,
        }),
        Err(error) => {
            let (suffix, reason) = match error {
                CompanyLogoError::TooLarge => ("size", format!("it must be at most {kib} KiB")),
                CompanyLogoError::InvalidDimensions => (
                    "dimensions",
                    format!("its dimensions must be between 1 and {max} pixels"),
                ),
                CompanyLogoError::Animated => ("animated", "it must be a still image".to_owned()),
                CompanyLogoError::UnsupportedFormat => (
                    "format",
                    "it must contain PNG, JPEG or WebP image bytes".to_owned(),
                ),
                CompanyLogoError::Malformed => {
                    ("format", "the image is malformed or truncated".to_owned())
                }
            };
            // The logo keeps its established messages.
            let message = if kind.code == LOGO.code {
                format!("{what} '{reference}': {error}")
            } else {
                format!("{what} '{reference}': {reason}")
            };
            failure(diagnostics, suffix, message);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVQI12P4DwQACfsD/WMmxY8AAAAASUVORK5CYII=";

    fn style(company: serde_json::Value) -> ResolvedStyle {
        let mut style = crate::resolve_styles(&["air".into()], &mut Vec::new());
        style.document["company"] = company;
        style
    }

    #[test]
    fn a_reports_logo_files_are_named_once_its_style_file_is_supplied() {
        let frontmatter = json!({"styles": ["company.rstyle.yaml"]});
        let mut options = RenderOptions::default();
        // The style file names the logos, so a host sent files one by one learns of them only
        // after it has supplied the style's text.
        assert!(report_company_logo_files(&frontmatter, &options).is_empty());
        options.data.insert(
            "company.rstyle.yaml".into(),
            "company:\n  name: Example\n  logo: logo.png\n  logo-dark: brand/logo-dark.png\n"
                .into(),
        );
        assert_eq!(
            report_company_logo_files(&frontmatter, &options),
            ["logo.png", "brand/logo-dark.png"]
        );
        // A style passed by the caller counts as in the render.
        let passed = RenderOptions {
            styles: vec!["company:\n  logo: mark.webp\n".into()],
            ..Default::default()
        };
        assert_eq!(
            report_company_logo_files(&json!({}), &passed),
            ["mark.webp"]
        );
    }

    #[test]
    fn resolves_by_bytes_and_uses_the_company_name() {
        let style = style(json!({"name":"Example & Co", "logo":"media/logo.bin"}));
        let data = BTreeMap::from([(
            "media/logo.bin".into(),
            format!("data:application/octet-stream;base64,{PNG}"),
        )]);
        let mut diagnostics = Vec::new();
        let brand = resolve_company_brand(&style, &data, &mut diagnostics);
        assert_eq!(brand.name, "Example & Co");
        assert_eq!(
            brand.logo.unwrap().data_uri(),
            format!("data:image/png;base64,{PNG}")
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn company_card_uses_the_resolved_accent_in_srgb() {
        let resolved = crate::resolve_styles(
            &["preset: brutalist\ncolors:\n  brand: '#0B6E69'\n".into()],
            &mut Vec::new(),
        );
        let brand = resolve_company_brand(&resolved, &BTreeMap::new(), &mut Vec::new());
        assert_eq!(brand.base.as_deref(), Some("brutalist"));
        assert_eq!(
            brand.accent,
            resolved.document["tokens"]["accent"]
                .as_str()
                .and_then(richview_chart::color::parse)
                .map(richview_chart::color::Oklch::hex)
        );
        assert!(brand.accent.is_some());
    }

    #[test]
    fn a_color_only_company_uses_air_for_its_card_unless_a_builtin_was_named() {
        let mut options = RenderOptions {
            styles: vec!["colors: {brand: '#0B6E69'}".into()],
            ..Default::default()
        };
        let frontmatter = json!({});
        assert_eq!(
            report_company_brand(&frontmatter, &options).base.as_deref(),
            Some("air")
        );
        options.styles.insert(0, "scientific".into());
        assert_eq!(report_company_brand(&frontmatter, &options).base, None);
    }

    #[test]
    fn missing_name_makes_a_logo_decorative_and_reports_a_warning() {
        let style = style(json!({"logo":"logo.png"}));
        let data = BTreeMap::from([("logo.png".into(), format!("data:image/png;base64,{PNG}"))]);
        let mut diagnostics = Vec::new();
        let brand = resolve_company_brand(&style, &data, &mut diagnostics);
        assert!(brand.name.is_empty());
        assert!(brand.logo.is_some());
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "style.logo-name" && d.level == "warning")
        );
        assert!(brand.header_html(false).contains("alt=\"\""));
    }

    #[test]
    fn header_escapes_the_name_and_distinguishes_dark_and_print_marks() {
        let style =
            style(json!({"name":"Example \"&\" Co", "logo":"logo.png", "logo-dark":"dark.png"}));
        let data = BTreeMap::from([
            ("logo.png".into(), format!("data:image/png;base64,{PNG}")),
            ("dark.png".into(), format!("data:image/png;base64,{PNG}")),
        ]);
        let brand = resolve_company_brand(&style, &data, &mut Vec::new());
        let html = brand.header_html(true);
        assert!(html.contains("rv-report__logo--dark rv-report__logo--has-dark"));
        assert_eq!(
            html.matches("alt=\"Example &quot;&amp;&quot; Co\"").count(),
            2
        );
        assert!(!html.contains("rv-report__logo--plate"));
        assert!(!brand.header_css().contains("rv-report__logo--plate"));
        let no_dark = CompanyBrand {
            logo_dark: None,
            ..brand
        };
        assert!(no_dark.header_html(true).contains("rv-report__logo--plate"));
        // The plate's colour comes with the brand, on screen only; the base stylesheet names none.
        assert!(
            no_dark
                .header_css()
                .contains("@media screen{.rv-report__logo--plate{background:oklch(1 0 0)}}")
        );
    }

    #[test]
    fn header_sizes_square_wordmark_and_wide_mark_for_screen_and_phone() {
        let mut brand = CompanyBrand {
            logo: Some(CompanyLogo {
                bytes: Vec::new(),
                mime: "image/png",
                width: 1,
                height: 1,
            }),
            ..Default::default()
        };
        let css = brand.header_css();
        assert!(css.contains("width:2.500rem;height:2.500rem"));
        assert!(css.contains("width:2.250rem;height:2.250rem"));
        brand.logo.as_mut().unwrap().width = 4;
        let css = brand.header_css();
        assert!(css.contains("width:9.000rem;height:2.250rem"));
        assert!(css.contains("width:8.100rem;height:2.025rem"));
        brand.logo.as_mut().unwrap().width = 12;
        let css = brand.header_css();
        assert!(css.contains("width:12.000rem;height:1.000rem"));
        assert!(css.contains("width:10.800rem;height:0.900rem"));
    }

    #[test]
    fn unavailable_unsafe_and_oversized_logos_are_reported_without_a_mark() {
        for (reference, contents, code) in [
            ("../outside.png", None, "style.logo-path"),
            ("https://example.invalid/logo.png", None, "style.logo-path"),
            ("missing.png", None, "style.logo-missing"),
            (
                "invalid.png",
                Some("data:image/png;base64,%%%".to_owned()),
                "style.logo-format",
            ),
            (
                "disguised.png",
                Some(format!(
                    "data:image/png;base64,{}",
                    STANDARD.encode(b"<svg xmlns='http://www.w3.org/2000/svg'/>")
                )),
                "style.logo-format",
            ),
            (
                "large.png",
                Some(format!(
                    "data:image/png;base64,{}",
                    STANDARD.encode(vec![0; crate::COMPANY_LOGO_MAX_BYTES + 1])
                )),
                "style.logo-size",
            ),
        ] {
            let style = style(json!({"name":"Acme", "logo":reference}));
            let data = contents
                .map(|value| BTreeMap::from([(reference.to_owned(), value)]))
                .unwrap_or_default();
            let mut diagnostics = Vec::new();
            assert!(
                resolve_company_brand(&style, &data, &mut diagnostics)
                    .logo
                    .is_none()
            );
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.code == code && d.level == "error"),
                "{diagnostics:?}"
            );
        }
    }

    #[test]
    fn dark_logo_requires_a_standard_logo_for_light_and_print() {
        let style = style(json!({"name":"Acme", "logo-dark":"dark.png"}));
        let data = BTreeMap::from([("dark.png".into(), format!("data:image/png;base64,{PNG}"))]);
        let mut diagnostics = Vec::new();
        let brand = resolve_company_brand(&style, &data, &mut diagnostics);
        assert!(brand.header_html(true).is_empty());
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "style.logo-missing" && d.path == "company.logo")
        );
    }
}
