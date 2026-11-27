//! Common public schema parse operations, implemented in Rust for generated host adapters.
use crate::{Diagnostic, Document};
use regex_lite::Regex;
use serde_json::{Value, json};
use std::sync::OnceLock;

/// The source-only request consumed by the report publication API.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PublishRequest {
    pub slug: String,
    pub source: String,
}

/// Build a request without rendering or requiring publication metadata. The publishing host
/// validates metadata, slug and authorization before sending it. The slug comes from, in order,
/// the explicit override, the frontmatter `slug`, then the title (T-REPORT-SLUG). Each source is
/// judged on its own: a problem in another frontmatter field never changes the slug, an authored
/// slug that breaks the grammar is a `publish.slug` error and is never rewritten, and there is
/// no fallback slug (D-TYP-SLUG-FALLBACK).
pub fn create_publish_request(
    source: &str,
    slug_override: Option<&str>,
) -> Result<PublishRequest, Vec<Diagnostic>> {
    let document = crate::parse(source)?;
    let slug = resolve_slug(&document, slug_override)?;
    Ok(PublishRequest {
        slug,
        source: source.to_owned(),
    })
}

fn slug_error(message: impl Into<String>, path: &str) -> Vec<Diagnostic> {
    vec![
        Diagnostic::new("error", "publish.slug", message, path)
            .expected(crate::SLUG_GRAMMAR)
            .fix("Set `slug:` in the frontmatter or pass --slug with a valid value"),
    ]
}

/// Choose the publication slug of a parsed document.
pub(crate) fn resolve_slug(
    document: &Document,
    slug_override: Option<&str>,
) -> Result<String, Vec<Diagnostic>> {
    if let Some(slug) = slug_override {
        return match crate::slug_problem(slug) {
            Some(problem) => Err(slug_error(problem, "slug")),
            None => Ok(slug.to_owned()),
        };
    }
    if document
        .diagnostics
        .iter()
        .any(|d| d.code == "frontmatter.type" && d.path == "frontmatter.slug")
    {
        return Err(slug_error(
            "frontmatter.slug is not text, so it cannot be used. Quote it, for example `slug: \"2026-review\"`, or remove it to derive the slug from the title.",
            "frontmatter.slug",
        ));
    }
    if let Some(slug) = document.frontmatter["slug"].as_str() {
        return match crate::slug_problem(slug) {
            Some(problem) => Err(slug_error(problem, "frontmatter.slug")),
            None => Ok(slug.to_owned()),
        };
    }
    match document.frontmatter["title"].as_str() {
        Some(title) => crate::derive_slug(title).ok_or_else(|| {
            slug_error(
                "The title has no Latin letters or digits to build a slug from. Set `slug:` in the frontmatter.",
                "frontmatter.title",
            )
        }),
        None => Err(slug_error(
            "There is no slug: the report has neither `slug` nor `title` in its frontmatter. Add one of them.",
            "frontmatter.slug",
        )),
    }
}

/// Validate and normalize the public schema vocabulary. Generated bindings provide the familiar
/// `.parse` / `.safeParse` methods; third-party validator combinators are outside this adapter.
pub fn parse_schema(name: &str, mut value: Value) -> Result<Value, Vec<Diagnostic>> {
    let tag = match name {
        "ChartSchema" | "chart" => "chart",
        "TableSchema" | "table" => "table",
        "BigSchema" | "big" => {
            return Err(vec![issue(
                "big",
                "The `big` block is now `metric` (TYP-09); rename the tag",
            )]);
        }
        "MetricSchema" | "metric" => "metric",
        "CalloutSchema" | "callout" => "callout",
        "ImageSchema" | "image" => "image",
        "EmbedSchema" | "embed" => "embed",
        "AudioSchema" | "audio" => "audio",
        "VideoSchema" | "video" => "video",
        "DocumentFrontmatterSchema" | "frontmatter" => {
            if !value.is_object() {
                return Err(vec![issue("frontmatter", "Expected a mapping")]);
            }
            let mut doc = Document {
                frontmatter: value,
                body: String::new(),
                blocks: vec![],
                diagnostics: vec![],
                block_placeholder_prefix: None,
                body_line: 0,
                frontmatter_lines: Default::default(),
                frontmatter_columns: Default::default(),
                failed: vec![],
                deck: None,
            };
            doc.diagnostics.extend(crate::walker::check(
                crate::walker::Root::Frontmatter,
                &mut doc.frontmatter,
            ));
            crate::parser::validate_frontmatter(&mut doc);
            // Keys the registry does not know pass through; everything else is refused.
            doc.diagnostics
                .retain(|d| d.code != "frontmatter.unknown-key");
            if !doc.diagnostics.is_empty() {
                return Err(doc.diagnostics);
            }
            return Ok(doc.frontmatter);
        }
        "DataSourceSchema" | "data" => {
            let mut diagnostics = vec![];
            validate_data(&value, "data", true, &mut diagnostics);
            if !diagnostics.is_empty() {
                return Err(diagnostics);
            }
            retain_keys(&mut value, &["inline", "csv", "json", "parquet"]);
            return Ok(value);
        }
        "versionString" | "version" => {
            return if let Some(version) = value.as_str().filter(|v| crate::is_valid_version(v)) {
                Ok(json!(version))
            } else {
                Err(vec![issue(
                    "version",
                    crate::version_problem(value.as_str().unwrap_or_default()).unwrap_or_else(
                        || {
                            format!(
                                "Version must be text. Quote it, for example \"{}\".",
                                crate::VERSION_GRAMMAR
                            )
                        },
                    ),
                )])
            };
        }
        "ChartCheckSchema" | "chart-check" => return parse_finding(value, false),
        "DroppedLabelSchema" | "dropped-label" => return parse_finding(value, true),
        "ChartDiagnosticsSchema" | "chart-diagnostics" => {
            return parse_diagnostics("ChartDiagnosticsSchema", value);
        }
        "RenderDiagnosticsSchema" | "render-diagnostics" => {
            return parse_diagnostics("RenderDiagnosticsSchema", value);
        }
        _ => return Err(vec![issue("schema", format!("Unknown schema '{name}'"))]),
    };
    // A flow chart written as cards brings its own Table rows, as in a report (FEA-CH-76).
    if tag == "chart" {
        crate::parser::flow_rows(&mut value);
    }
    // The registry's walk names and drops unknown keys; its warnings do not fail the adapter.
    let mut diagnostics: Vec<Diagnostic> =
        crate::walker::check(crate::walker::Root::Block(tag), &mut value)
            .into_iter()
            .filter(|d| d.level == "error")
            .collect();
    let walked: Vec<String> = diagnostics.iter().map(|d| d.path.clone()).collect();
    diagnostics.extend(
        crate::parser::validate_block(tag, &value, 1)
            .into_iter()
            .filter(|d| !walked.contains(&d.path)),
    );
    if let Some(data) = value.get("data") {
        validate_data(data, &format!("{tag}.data"), false, &mut diagnostics);
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    apply_defaults(tag, &mut value);
    if let Some(data) = value.get_mut("data") {
        retain_keys(data, &["inline", "csv", "json", "parquet"]);
    }
    Ok(value)
}

/// Whether `date` is `YYYY-MM-DD` or an RFC 3339 timestamp.
pub(crate) fn date_shape(date: &str) -> bool {
    static DATE: OnceLock<Regex> = OnceLock::new();
    DATE.get_or_init(|| {
        Regex::new(
            r"^\d{4}-\d{2}-\d{2}(?:[Tt ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:[Zz]|[+-]\d{2}:\d{2}))?$",
        )
        .expect("date grammar")
    })
    .is_match(date.trim())
}

/// Whether `tag` is a well-formed BCP 47 language tag (language, optional script, region and
/// variants; no extensions).
pub(crate) fn language_tag(tag: &str) -> bool {
    static LANG: OnceLock<Regex> = OnceLock::new();
    LANG.get_or_init(|| {
        Regex::new(r"^[A-Za-z]{2,3}(?:-[A-Za-z]{4})?(?:-(?:[A-Za-z]{2}|[0-9]{3}))?(?:-(?:[A-Za-z0-9]{5,8}|[0-9][A-Za-z0-9]{3}))*$")
            .expect("language grammar")
    })
    .is_match(tag)
}

/// The major version of an `rmd/<major>` format marker.
pub(crate) fn format_major(marker: &str) -> Option<u32> {
    marker.trim().strip_prefix("rmd/")?.parse().ok()
}

/// Source parsing and public schema adapters share defaults without cloning decoded row trees.
pub(crate) fn apply_defaults(tag: &str, value: &mut Value) {
    let Some(map) = value.as_object_mut() else {
        return;
    };
    match tag {
        "table" => {
            map.entry("max_rows").or_insert(json!(20));
            map.entry("total").or_insert(json!(false));
            if let Some(sort) = map.get_mut("sort").and_then(Value::as_object_mut) {
                sort.entry("dir").or_insert(json!("asc"));
            }
        }
        "image" => {
            map.entry("fit").or_insert(json!("contain"));
        }
        "embed" => {
            map.entry("height").or_insert(json!(420));
            map.entry("allow")
                .or_insert(json!(crate::registry::EMBED_HOSTS));
        }
        _ => {}
    }
}

fn issue(path: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new("error", "schema.invalid", message, path)
}
fn retain_keys(value: &mut Value, keys: &[&str]) {
    if let Some(map) = value.as_object_mut() {
        map.retain(|key, _| keys.contains(&key.as_str()));
    }
}

fn validate_data(
    value: &Value,
    path: &str,
    document_source: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(map) = value.as_object() else {
        diagnostics.push(issue(path, "Expected a data mapping"));
        return;
    };
    for key in ["id", "csv", "json", "parquet", "url", "sql"] {
        if map.get(key).is_some_and(|v| !v.is_string()) {
            diagnostics.push(issue(&format!("{path}.{key}"), "Expected a string"));
        }
    }
    if map.get("inline").is_some_and(|v| {
        !v.as_array()
            .is_some_and(|rows| rows.iter().all(Value::is_object))
    }) {
        diagnostics.push(issue(
            &format!("{path}.inline"),
            "Expected an array of row mappings",
        ));
    }
    let _ = document_source;
    for key in ["id", "url", "sql", "headers", "cache"] {
        if map.contains_key(key) {
            diagnostics.push(issue(path,"Use block-local data.inline or a bounded csv/json path; header IDs and remote/SQL sources are unsupported."));
        }
    }
    for key in crate::document::DATA_FILE_KEYS {
        if let Some(path) = map.get(key).and_then(Value::as_str)
            && !crate::report_relative_path(path)
        {
            diagnostics.push(issue(
                path,
                "Use a relative file inside the dedicated report folder.",
            ));
        }
    }
}

fn parse_finding(mut value: Value, dropped: bool) -> Result<Value, Vec<Diagnostic>> {
    let mut diagnostics = vec![];
    if !value.is_object() {
        return Err(vec![issue("finding", "Expected a finding mapping")]);
    }
    let required = if dropped {
        &["text"][..]
    } else {
        &["kind", "message"][..]
    };
    for key in required {
        if !value[key].is_string() {
            diagnostics.push(issue(key, "Expected a string"));
        }
    }
    let enum_valid = if dropped {
        matches!(
            value["reason"].as_str(),
            Some("collision" | "out-of-bounds" | "density")
        )
    } else {
        matches!(
            value["severity"].as_str(),
            Some("error" | "warning" | "info")
        )
    };
    if !enum_valid {
        diagnostics.push(issue(
            if dropped { "reason" } else { "severity" },
            "Expected a known finding severity/reason",
        ));
    }
    if dropped {
        if value.get("collidedWith").is_some_and(|v| !v.is_string()) {
            diagnostics.push(issue("collidedWith", "Expected label identifier string"));
        }
        if let Some(point) = value.get("wouldBeAt") {
            for key in ["x", "y"] {
                if !point[key].is_number() {
                    diagnostics.push(issue(&format!("wouldBeAt.{key}"), "Expected a number"));
                }
            }
        }
    } else {
        for key in ["metric", "threshold"] {
            if value.get(key).is_some_and(|v| !v.is_number()) {
                diagnostics.push(issue(key, "Expected a number"));
            }
        }
        if value
            .get("related")
            .is_some_and(|v| !v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
        {
            diagnostics.push(issue("related", "Expected an array of identifiers"));
        }
        if let Some(region) = value.get("region") {
            for key in ["x", "y", "w", "h"] {
                if !region[key].is_number() {
                    diagnostics.push(issue(&format!("region.{key}"), "Expected a number"));
                }
            }
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    retain_keys(
        &mut value,
        if dropped {
            &["text", "reason", "wouldBeAt", "collidedWith"]
        } else {
            &[
                "kind",
                "severity",
                "message",
                "metric",
                "threshold",
                "region",
                "related",
            ]
        },
    );
    Ok(value)
}

fn parse_diagnostics(name: &str, value: Value) -> Result<Value, Vec<Diagnostic>> {
    let mut diagnostics = vec![];
    let status_valid = matches!(value["status"].as_str(), Some("clean" | "issues"));
    if !value.is_object() || !status_valid {
        diagnostics.push(issue("status", "Expected clean or issues status"));
    }
    if name == "RenderDiagnosticsSchema" {
        if !value["parse"].as_array().is_some_and(|rows| {
            rows.iter().all(|d| {
                d.is_object()
                    && matches!(d["level"].as_str(), Some("error" | "warning" | "info"))
                    && d["message"].is_string()
            })
        }) {
            diagnostics.push(issue("parse", "Expected source diagnostics"));
        }
        if let Some(charts) = value["charts"].as_array() {
            for chart in charts {
                if let Err(mut errors) = parse_diagnostics("ChartDiagnosticsSchema", chart.clone())
                {
                    diagnostics.append(&mut errors);
                }
            }
        } else {
            diagnostics.push(issue("charts", "Expected chart diagnostics array"));
        }
    } else {
        for key in ["chartId", "chartType", "engine"] {
            if !value[key].is_string() {
                diagnostics.push(issue(key, "Expected a string"));
            }
        }
        if !matches!(
            value["engine"].as_str(),
            Some("rust-svg" | "rust-time-series" | "rust")
        ) {
            diagnostics.push(issue("engine", "Expected a known chart engine"));
        }
        if !value["measured"].is_boolean() {
            diagnostics.push(issue("measured", "Expected a boolean"));
        }
        if let Some(checks) = value["checks"].as_array() {
            for check in checks {
                if let Err(mut errors) = parse_finding(check.clone(), false) {
                    diagnostics.append(&mut errors);
                }
            }
        } else {
            diagnostics.push(issue("checks", "Expected chart check findings"));
        }
        if let Some(labels) = value["droppedLabels"].as_array() {
            for label in labels {
                if let Err(mut errors) = parse_finding(label.clone(), true) {
                    diagnostics.append(&mut errors);
                }
            }
        } else {
            diagnostics.push(issue("droppedLabels", "Expected omitted label provenance"));
        }
        if let Some(dimensions) = value.get("dimensions") {
            for key in ["width", "height"] {
                if !dimensions[key].is_number() {
                    diagnostics.push(issue(&format!("dimensions.{key}"), "Expected a number"));
                }
            }
        }
    }
    if diagnostics.is_empty() {
        Ok(value)
    } else {
        Err(diagnostics)
    }
}
