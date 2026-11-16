//! Observed distributions: R-7 quartiles, inclusive 1.5-IQR whiskers and original outliers.
use super::*;
use crate::Diagnostic;

/// Source parsing and direct chart handles share these option checks.
pub fn options(chart: &Value) -> Result<(), Diagnostic> {
    if get(chart, "/encoding/y", "").is_empty() {
        return Err(Diagnostic::error(
            "/encoding/y",
            "boxplot encoding.y requires a numeric observation field",
        ));
    }
    for channel in ["size", "label"] {
        if chart["encoding"].get(channel).is_some() {
            return Err(Diagnostic::error(
                format!("/encoding/{channel}"),
                format!("boxplot encoding.{channel} is unsupported; each accepted row contributes one observation"),
            ));
        }
    }
    if chart
        .pointer("/style/orientation")
        .is_some_and(|value| !matches!(value.as_str(), Some("auto" | "vertical" | "horizontal")))
    {
        return Err(Diagnostic::error(
            "/style/orientation",
            "boxplot style.orientation must be auto, vertical or horizontal",
        ));
    }
    for key in ["min", "max"] {
        if chart["axes"]["y"]
            .get(key)
            .is_some_and(|value| value.as_f64().is_none_or(|value| !value.is_finite()))
        {
            return Err(Diagnostic::error(
                format!("/axes/y/{key}"),
                "boxplot value-axis bounds must be finite numbers",
            ));
        }
    }
    if let (Some(lo), Some(hi)) = (chart["axes"]["y"]["min"].as_f64(), chart["axes"]["y"]["max"].as_f64())
        && lo >= hi
    {
        return Err(Diagnostic::error(
            "/axes/y",
            "boxplot axes.y.min must be less than axes.y.max",
        ));
    }
    Ok(())
}
