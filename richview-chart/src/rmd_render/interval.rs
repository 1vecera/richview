//! Authored bounds and optional estimates; no distribution or confidence is inferred.
use super::*;
use crate::Diagnostic;

/// Source parsing and direct chart handles share the same option contract.
pub fn options(chart: &Value) -> Result<(), Diagnostic> {
    for channel in ["x", "lower", "upper"] {
        if chart["encoding"][channel].as_str().is_none_or(str::is_empty) {
            return Err(Diagnostic::error(
                format!("/encoding/{channel}"),
                format!("interval encoding.{channel} requires a field name"),
            ));
        }
    }
    for channel in ["y", "color", "facet"] {
        if chart["encoding"]
            .get(channel)
            .is_some_and(|value| value.as_str().is_none_or(str::is_empty))
        {
            return Err(Diagnostic::error(
                format!("/encoding/{channel}"),
                format!("interval encoding.{channel} must name a field when supplied"),
            ));
        }
    }
    for channel in ["size", "label"] {
        if chart["encoding"].get(channel).is_some() {
            return Err(Diagnostic::error(
                format!("/encoding/{channel}"),
                format!("interval encoding.{channel} is unsupported; each accepted row supplies one range"),
            ));
        }
    }
    if chart.get("binning").is_some() {
        return Err(Diagnostic::error(
            "/binning",
            "interval charts use authored bounds, not bins",
        ));
    }
    if chart
        .pointer("/style/orientation")
        .is_some_and(|value| !matches!(value.as_str(), Some("auto" | "vertical" | "horizontal")))
    {
        return Err(Diagnostic::error(
            "/style/orientation",
            "interval style.orientation must be auto, vertical or horizontal",
        ));
    }
    for key in ["min", "max"] {
        if chart["axes"]["y"]
            .get(key)
            .is_some_and(|value| value.as_f64().is_none_or(|value| !value.is_finite()))
        {
            return Err(Diagnostic::error(
                format!("/axes/y/{key}"),
                "interval value-axis bounds must be finite numbers",
            ));
        }
    }
    if let (Some(lo), Some(hi)) = (chart["axes"]["y"]["min"].as_f64(), chart["axes"]["y"]["max"].as_f64())
        && lo >= hi
    {
        return Err(Diagnostic::error(
            "/axes/y",
            "interval axes.y.min must be less than axes.y.max",
        ));
    }
    Ok(())
}
