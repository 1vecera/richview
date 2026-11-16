//! Source histograms: statistical bins retain their original observations in the payload.
use super::*;
use crate::Diagnostic;
use crate::binning::Spec;

/// The same histogram-specific options are checked by source validation and direct rendering.
pub fn options(chart: &Value) -> Result<Spec, Diagnostic> {
    if get(chart, "/encoding/x", "").is_empty() {
        return Err(Diagnostic::error(
            "/encoding/x",
            "histogram encoding.x requires a numeric observation field",
        ));
    }
    for channel in ["y", "size", "label"] {
        if chart["encoding"].get(channel).is_some() {
            return Err(Diagnostic::error(
                format!("/encoding/{channel}"),
                format!("histogram encoding.{channel} is unsupported; each accepted row contributes one observation"),
            ));
        }
    }
    if chart
        .pointer("/style/orientation")
        .is_some_and(|value| !matches!(value.as_str(), Some("auto" | "vertical")))
    {
        return Err(Diagnostic::error(
            "/style/orientation",
            "histogram style.orientation must be auto or vertical",
        ));
    }
    if chart
        .pointer("/axes/y/min")
        .is_some_and(|value| number(value) != Some(0.0))
    {
        return Err(Diagnostic::error(
            "/axes/y/min",
            "histogram axes.y.min must be zero so interval heights retain their meaning",
        ));
    }
    if chart
        .pointer("/axes/y/max")
        .is_some_and(|value| number(value).is_none_or(|value| value <= 0.0))
    {
        return Err(Diagnostic::error(
            "/axes/y/max",
            "histogram axes.y.max must be a positive finite number",
        ));
    }
    let spec: Spec = match chart.get("binning") {
        Some(value) => {
            serde_json::from_value(value.clone()).map_err(|error| Diagnostic::error("/binning", error.to_string()))?
        }
        None => Spec::default(),
    };
    spec.validate()
        .map_err(|message| Diagnostic::error("/binning", message))?;
    Ok(spec)
}
