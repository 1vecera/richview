//! RichView's target-independent Rich Markdown engine.
//!
//! Parsing, styles, rendering, diagnostics and authoring knowledge are pure: callers resolve
//! external references and supply their bytes through [`document::RenderOptions`]. The same crate runs
//! natively and in WebAssembly without a filesystem, network, process or JavaScript engine.
//! Hosts use the [`document`] API for parsing, resource validation, rendering and checks.
//!
//! Native builds compile the bundled font catalogs in. WebAssembly builds leave them out, so the
//! runtime that interactive reports embed stays small; a host that renders reports there loads
//! the staged catalog and provides it once with `provide_font_catalog`.

mod asset;
mod authoring;
mod blocks;
mod columns;
mod company_brand;
mod company_logo;
mod compat;
pub mod document;
mod fences;
mod figure;
mod folder;
mod heat;
mod highlight;
mod identity;
mod knowledge;
mod markdown;
// Legacy source rewrites are internal fixtures; no host uses a migration entry point.
#[cfg(test)]
mod migrate;
mod parser;
mod reading;
pub mod registry;
mod render;
mod scrolly;
mod shell;
mod slides;
pub mod spec;
mod style;
mod walker;
pub mod yaml;

pub(crate) use document::*;
use serde_json::Value;

pub use asset::{AssetRef, AssetUse, Visibility, is_loopback, media_type_for_extension};
pub use authoring::{
    CHART_TYPES, authoring_fields, authoring_keys, authoring_values, primary_style,
    with_primary_style,
};
pub use blocks::{image_color, render_table};
pub use company_brand::{
    CompanyBrand, CompanyLogo, report_company_brand, report_company_logo_files,
    report_style_media_files, resolve_company_brand,
};
pub use company_logo::{
    BACKGROUND_PICTURE_MAX_BYTES, BACKGROUND_PICTURE_MAX_DIMENSION, COMPANY_LOGO_MAX_BYTES,
    COMPANY_LOGO_MAX_DIMENSION, CompanyLogoError, ValidatedCompanyLogo, validate_company_logo,
};
pub use compat::{PublishRequest, create_publish_request, parse_schema};
pub use folder::{ASSET_FINDINGS, unsplash_image_url};
pub use highlight::{HighlightLanguage, highlight_html};
pub use identity::{SLUG_GRAMMAR, slug_problem, version_problem};
pub(crate) use identity::{VERSION_GRAMMAR, derive_slug, is_valid_version};
pub use parser::RICH_TAGS;
pub use render::render_compat;
#[cfg(not(target_arch = "wasm32"))]
pub use style::bundled_font_catalog;
pub use style::checks as style_checks;
pub use style::presets::{EXTENDED_PRESETS, is_preset, preset_names, recipe as preset_recipe};
#[cfg(target_arch = "wasm32")]
pub use style::provide_font_catalog;
pub use style::settings::{SETTINGS as STEP_SETTINGS, STEPS, StepSetting};
pub use style::{
    Grammar as StyleGrammar, Leaf as StyleLeaf, Mark as StyleMark, ResolvedStyle, brand_font_files,
    builtin_style_names, builtin_style_source, custom_properties_to_css, is_retired_style_name,
    is_style_name, report_background_layer, resolve_styles, resolve_styles_compat,
    style_list_problem, style_vocabulary,
};
pub(crate) use style::{MAX_STYLE_BYTES, MAX_STYLE_REFERENCES};

/// Top, side and bottom page margins in millimetres.
pub(crate) const PAGE_MARGINS_MM: [f64; 3] = [20.0, 18.0, 22.0];

pub(crate) fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}

pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        out.push_str(match c {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            '\'' => "&#39;",
            _ => {
                out.push(c);
                continue;
            }
        });
    }
    out
}

pub(crate) fn json_script(value: &Value) -> String {
    // HTML's script raw-text tokenizer recognizes closing tags before JSON is parsed.
    value
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}
