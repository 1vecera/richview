//! Resolve an authored company's validated logo from host-supplied report resources.
use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::STANDARD};

use crate::{
    COMPANY_LOGO_MAX_BYTES, CompanyLogoError, Diagnostic, RenderOptions, ResolvedStyle,
    report_relative_path, validate_company_logo,
};

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
    CompanyBrand {
        name,
        base: company["base"].as_str().map(str::to_owned),
        accent: company.as_object().and_then(|_| {
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
    let mut sources: Vec<String> = frontmatter["styles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter_map(|reference| {
            if crate::is_style_name(reference) {
                Some(reference.to_owned())
            } else {
                options.data.get(reference).cloned()
            }
        })
        .collect();
    sources.extend(options.styles.iter().cloned());
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
        && style.document["company"]["color"].is_string()
        && !sources
            .iter()
            .any(|source| crate::builtin_style_names().contains(&source.as_str()))
    {
        company.base = Some("air".into());
    }
    company
}

fn read_logo(
    reference: Option<&str>,
    key: &str,
    data: &BTreeMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CompanyLogo> {
    let reference = reference?;
    let path = format!("company.{key}");
    let failure = |diagnostics: &mut Vec<Diagnostic>, code: &str, message: String| {
        let fix = match code {
            "style.logo-path" => "Use a file path inside the report folder",
            "style.logo-missing" => "Put the logo file inside the report folder",
            "style.logo-size" => "Use a logo no larger than 512 KiB",
            "style.logo-dimensions" => "Use a logo no larger than 1024 pixels on either side",
            "style.logo-animated" => "Use a still PNG, JPEG or WebP logo",
            _ => "Use a valid still PNG, JPEG or WebP logo",
        };
        diagnostics.push(Diagnostic::new("error", code, message, path.clone()).fix(fix));
    };
    if !report_relative_path(reference) {
        failure(
            diagnostics,
            "style.logo-path",
            "The company logo must be a report-relative file".into(),
        );
        return None;
    }
    let Some(uri) = data.get(reference) else {
        failure(
            diagnostics,
            "style.logo-missing",
            format!("Company logo '{reference}' is not available in the report folder"),
        );
        return None;
    };
    let Some((prefix, encoded)) = uri.split_once(";base64,") else {
        failure(
            diagnostics,
            "style.logo-format",
            format!("Company logo '{reference}' is not a supplied raster file"),
        );
        return None;
    };
    if !prefix.starts_with("data:") {
        failure(
            diagnostics,
            "style.logo-format",
            format!("Company logo '{reference}' is not a supplied raster file"),
        );
        return None;
    }
    // The host retains at most one byte beyond the limit. Direct callers get the same bound.
    if encoded.len() > (COMPANY_LOGO_MAX_BYTES + 1).div_ceil(3) * 4 {
        failure(
            diagnostics,
            "style.logo-size",
            format!("Company logo '{reference}' exceeds 512 KiB"),
        );
        return None;
    }
    let Ok(bytes) = STANDARD.decode(encoded) else {
        failure(
            diagnostics,
            "style.logo-format",
            format!("Company logo '{reference}' has invalid encoded bytes"),
        );
        return None;
    };
    match validate_company_logo(&bytes) {
        Ok(image) => Some(CompanyLogo {
            bytes,
            mime: image.mime,
            width: image.width,
            height: image.height,
        }),
        Err(error) => {
            let code = match error {
                CompanyLogoError::TooLarge => "style.logo-size",
                CompanyLogoError::InvalidDimensions => "style.logo-dimensions",
                CompanyLogoError::Animated => "style.logo-animated",
                CompanyLogoError::UnsupportedFormat | CompanyLogoError::Malformed => {
                    "style.logo-format"
                }
            };
            failure(
                diagnostics,
                code,
                format!("Company logo '{reference}': {error}"),
            );
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
            &["company:\n  color: '#0B6E69'\n  base: brutalist\n".into()],
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
            styles: vec!["company: {color: '#0B6E69'}".into()],
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
                    STANDARD.encode(vec![0; COMPANY_LOGO_MAX_BYTES + 1])
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
