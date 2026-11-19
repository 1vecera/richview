//! The five-step settings of STY-73: every author setting other than colours, fonts and the
//! colophon takes `xsmall`, `small`, `medium`, `large` or `xlarge`.
//!
//! `medium` is always the preset's own value, so a step means the same change in every preset
//! and a setting left out changes nothing. Steps multiply the preset's values and never add, so
//! the type scale keeps its ratios; caps keep every extreme combination readable. The colour
//! steps (`tint`, `gridlines`) are applied by the colour derivation, after these, so no step can
//! push text below its contrast floor.

use serde_json::{Map, Value, json};

/// The five steps, smallest first. `medium` is the preset's own value.
pub const STEPS: [&str; 5] = ["xsmall", "small", "medium", "large", "xlarge"];

/// The index of the medium step.
pub const MEDIUM: usize = 2;

/// One five-step setting: its key and what it changes.
#[derive(Debug, Clone, Copy)]
pub struct StepSetting {
    pub name: &'static str,
    pub about: &'static str,
}

/// Every five-step setting, in the order the docs teach them.
pub const SETTINGS: [StepSetting; 8] = [
    StepSetting {
        name: "text-size",
        about: "The whole type scale: x0.8, x0.9, the preset's, x1.1, x1.2",
    },
    StepSetting {
        name: "spacing",
        about: "Space between blocks and before sections: x0.5, x0.75, the preset's, x1.33, x1.67",
    },
    StepSetting {
        name: "padding",
        about: "Space inside blocks (paragraphs, lists, table cells, callouts, metrics, framed charts): x0.75, x0.875, the preset's, x1.25, x1.5",
    },
    StepSetting {
        name: "corners",
        about: "Corners of images, embeds, code, the readout and bar ends: square, half, the preset's, rounder, roundest",
    },
    StepSetting {
        name: "lines",
        about: "Weight of the header bars, callout and metric rules: x0.5, x0.75, the preset's, x1.5, x2; hairlines stay at least 1px",
    },
    StepSetting {
        name: "gridlines",
        about: "How strongly the chart's soft rules show (gridlines, and the tracks and empty cells some charts draw): faintest, fainter, the preset's, stronger, strongest; never invisible",
    },
    StepSetting {
        name: "tint",
        about: "How much of the brand hue the page, rules and text carry: grey, half, the preset's, x2, x3.5",
    },
    StepSetting {
        name: "motion",
        about: "How far and how long blocks and charts move: still, quicker, the preset's, slower, most",
    },
];

/// The step a word names.
pub fn step_index(word: &str) -> Option<usize> {
    STEPS.iter().position(|step| *step == word)
}

/// The step of `name` in the merged document's `settings.steps`, medium when unset.
pub(super) fn step(merged: &Value, name: &str) -> usize {
    merged
        .pointer(&format!("/settings/steps/{name}"))
        .and_then(Value::as_u64)
        .map_or(MEDIUM, |index| (index as usize).min(STEPS.len() - 1))
}

/// Multiplier of the type scale per step.
const TEXT: [f64; 5] = [0.8, 0.9, 1.0, 1.1, 1.2];
/// Multiplier of the space between blocks per step.
const SPACING: [f64; 5] = [0.5, 0.75, 1.0, 4.0 / 3.0, 5.0 / 3.0];
/// Multiplier of the space inside blocks per step.
const PADDING: [f64; 5] = [0.75, 0.875, 1.0, 1.25, 1.5];
/// Multiplier of structural rule weights per step.
const LINES: [f64; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];
/// Motion: duration, distance and stagger multipliers per step; `xsmall` is still.
const MOTION: [(f64, f64, f64); 5] = [
    (0.0, 0.0, 0.0),
    (0.6, 0.5, 0.5),
    (1.0, 1.0, 1.0),
    (1.2, 1.3, 1.25),
    (1.4, 1.6, 1.5),
];
/// Multiplier of the neutrals' chroma per `tint` step.
pub(super) const TINT: [f64; 5] = [0.0, 0.5, 1.0, 2.0, 3.5];
/// Multiplier of the chart's soft rules' contrast above 1:1 per `gridlines` step; the faintest
/// step keeps them visible (`brand.rs`'s floor), since they also draw tracks and empty cells.
pub(super) const GRIDLINES: [f64; 5] = [0.3, 0.6, 1.0, 1.8, 2.6];

/// The type factor on phones stays within these bounds, so phone body text stays readable at
/// `xsmall` and phone titles fit at `xlarge`.
const PHONE_TEXT: (f64, f64) = (0.9, 1.1);
/// Gaps between blocks never close below these, in rem: desktop and phone.
const MIN_GAP: (f64, f64) = (1.25, 1.0);
/// Corner caps in px: images and embeds, code and the readout, bar ends.
const CORNER_CAP: (f64, f64, f64) = (24.0, 10.0, 6.0);
/// Motion caps: duration in ms, distance in px, stagger in ms.
const MOTION_CAP: (f64, f64, f64) = (1000.0, 28.0, 120.0);

/// A length's number and unit, for `px` and `rem` values and bare numbers.
fn length(value: &Value) -> Option<(f64, &'static str)> {
    if let Some(number) = value.as_f64() {
        return Some((number, ""));
    }
    let text = value.as_str()?;
    for unit in ["px", "rem"] {
        if let Some(number) = text.strip_suffix(unit).and_then(|n| n.parse::<f64>().ok()) {
            return Some((number, unit));
        }
    }
    text.parse::<f64>().ok().map(|number| (number, ""))
}

fn write_length(number: f64, unit: &str) -> Value {
    let text = format!("{number:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if unit.is_empty() {
        json!(text.parse::<f64>().unwrap_or(number))
    } else {
        json!(format!("{text}{unit}"))
    }
}

/// A rounded corner at `step` for a preset value `own`, capped; a preset already above the cap
/// keeps its own value at the larger steps.
pub(super) fn corner(own: f64, step: usize, small_gain: f64, large_gain: f64, cap: f64) -> f64 {
    let value = match step {
        0 => 0.0,
        1 => own / 2.0,
        MEDIUM => return own,
        3 => (own * 1.5).max(own + small_gain),
        _ => (own * 2.0).max(own + large_gain),
    };
    if step > MEDIUM {
        value.min(cap).max(own)
    } else {
        value
    }
}

/// Apply the non-colour steps to the merged cascade. Run after every layer is merged and before
/// the colour derivation.
pub(super) fn apply(merged: &mut Value) {
    let text = step(merged, "text-size");
    let spacing = step(merged, "spacing");
    let padding = step(merged, "padding");
    let corners = step(merged, "corners");
    let lines = step(merged, "lines");
    let motion = step(merged, "motion");
    if !merged["tokens"].is_object() {
        merged["tokens"] = json!({});
    }
    let tokens = merged["tokens"]
        .as_object_mut()
        .expect("tokens is a mapping");
    if text != MEDIUM {
        let factor = TEXT[text];
        let phone = factor.clamp(PHONE_TEXT.0, PHONE_TEXT.1);
        scale_sizes(tokens, factor, phone);
    }
    if text != MEDIUM || spacing != MEDIUM {
        // Space between blocks follows the type, so bigger type gets proportionally bigger gaps.
        let text_factor = TEXT[text];
        let phone_text = text_factor.clamp(PHONE_TEXT.0, PHONE_TEXT.1);
        let gap = SPACING[spacing] * text_factor;
        let phone_gap = (1.0 + (SPACING[spacing] - 1.0) / 2.0) * phone_text;
        for (key, factor, floor) in [
            ("block-gap", gap, MIN_GAP.0),
            ("block-gap-phone", phone_gap, MIN_GAP.1),
        ] {
            if let Some((number, unit)) = tokens.get(key).and_then(length) {
                let scaled = number * factor;
                let floor = if unit == "px" { floor * 16.0 } else { floor };
                // A preset already tighter than the floor keeps its own gap.
                let scaled = if factor < 1.0 {
                    scaled.max(floor.min(number))
                } else {
                    scaled
                };
                tokens.insert(key.into(), write_length(scaled, unit));
            }
        }
    }
    if padding != MEDIUM
        && let Some((number, _)) = tokens.get("density").and_then(length)
    {
        tokens.insert(
            "density".into(),
            write_length((number * PADDING[padding]).clamp(0.5, 2.0), ""),
        );
    }
    if corners != MEDIUM {
        for (key, cap, small, large) in [
            ("radius-m", CORNER_CAP.0, 4.0, 10.0),
            ("radius-s", CORNER_CAP.1, 2.0, 6.0),
        ] {
            if let Some((number, unit)) = tokens.get(key).and_then(length) {
                let unit = if unit.is_empty() { "px" } else { unit };
                tokens.insert(
                    key.into(),
                    write_length(corner(number, corners, small, large, cap), unit),
                );
            }
        }
        if let Some(own) = merged.pointer("/chart/bar-radius").and_then(Value::as_f64) {
            merged["chart"]["bar-radius"] =
                write_length(corner(own, corners, 2.0, 4.0, CORNER_CAP.2), "");
        }
    }
    if lines != MEDIUM {
        for pointer in [
            "/components/callout/border-width",
            "/components/header/border-top",
            "/components/header/border-bottom",
        ] {
            let Some((number, unit)) = merged.pointer(pointer).and_then(length) else {
                continue;
            };
            if number <= 0.0 {
                continue;
            }
            let unit = if unit.is_empty() { "px" } else { unit };
            // Rules stay at least one pixel and land on half pixels.
            let scaled = ((number * LINES[lines]) * 2.0).round() / 2.0;
            let scaled = if unit == "px" {
                scaled.max(1.0)
            } else {
                scaled
            };
            if let Some(slot) = merged.pointer_mut(pointer) {
                *slot = write_length(scaled, unit);
            }
        }
    }
    if motion != MEDIUM {
        if !merged["motion"].is_object() {
            merged["motion"] = json!({});
        }
        let section = merged["motion"]
            .as_object_mut()
            .expect("motion is a mapping");
        if motion == 0 {
            section.insert("enabled".into(), json!(false));
        } else {
            let (duration, distance, stagger) = MOTION[motion];
            scale_number(section, "duration", duration, MOTION_CAP.0);
            scale_number(section, "distance", distance, MOTION_CAP.1);
            scale_number(section, "stagger", stagger, MOTION_CAP.2);
        }
    }
}

/// The padding factor of a framed chart: shaded and outlined figures pad by 1rem and 1.25rem a
/// side at `medium`, and the layout insets the plot by the same amount (`frame_inset_px`).
pub(crate) fn frame_padding(document: &Value) -> f64 {
    PADDING[step(document, "padding")]
}

/// CSS the steps add beyond the tokens: a framed chart's padding (`padding`) and the metric's
/// rules (`lines`). At `medium` it is empty, so a report without steps is byte for byte as before.
pub(super) fn css(merged: &Value) -> String {
    let mut css = String::new();
    let padding = step(merged, "padding");
    if padding != MEDIUM {
        let f = PADDING[padding];
        let rem = |n: f64| {
            let text = format!("{:.3}", n * f);
            format!("{}rem", text.trim_end_matches('0').trim_end_matches('.'))
        };
        match merged
            .pointer("/components/figure/frame")
            .and_then(Value::as_str)
        {
            Some("shaded" | "outline") => css.push_str(&format!(
                ".rv-chart{{padding:{} {}}}\n",
                rem(1.0),
                rem(1.25)
            )),
            Some("rule-above") => {
                css.push_str(&format!(".rv-chart{{padding-top:{}}}\n", rem(0.75)));
            }
            _ => {}
        }
    }
    let lines = step(merged, "lines");
    if lines != MEDIUM {
        let px = |n: f64| {
            let scaled = ((n * LINES[lines]) * 2.0).round() / 2.0;
            let text = format!("{:.1}", scaled.max(1.0));
            format!("{}px", text.trim_end_matches('0').trim_end_matches('.'))
        };
        match merged
            .pointer("/components/metric/style")
            .and_then(Value::as_str)
        {
            Some("knockout") => {
                css.push_str(&format!(".rv-metric{{border-width:{}}}\n", px(2.0)));
            }
            Some("underline") => {}
            _ => css.push_str(&format!(
                ".rv-metric{{border-top-width:{0};border-bottom-width:{0}}}\n",
                px(1.0)
            )),
        }
    }
    css
}

/// Multiply every type size; phone sizes take the phone factor.
fn scale_sizes(tokens: &mut Map<String, Value>, factor: f64, phone: f64) {
    for (key, value) in tokens.iter_mut() {
        if !super::size_token(key) {
            continue;
        }
        let factor = if key.ends_with(super::PHONE_SUFFIX) {
            phone
        } else {
            factor
        };
        if let Some(text) = value.as_str() {
            *value = json!(super::scale_length(text, factor));
        }
    }
}

fn scale_number(section: &mut Map<String, Value>, key: &str, factor: f64, cap: f64) {
    if let Some(number) = section.get(key).and_then(Value::as_f64) {
        let scaled = (number * factor).min(cap.max(number)).round();
        section.insert(key.into(), json!(scaled));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_is_monotonic_and_medium_is_one() {
        for table in [TEXT, SPACING, PADDING, LINES, TINT, GRIDLINES] {
            assert_eq!(table[MEDIUM], 1.0);
            assert!(table.windows(2).all(|pair| pair[0] < pair[1]), "{table:?}");
        }
        let motion: Vec<f64> = MOTION.iter().map(|m| m.0).collect();
        assert!(motion.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn corners_grow_with_the_step_and_respect_the_cap() {
        for own in [0.0, 2.0, 3.0, 12.0, 30.0] {
            let values: Vec<f64> = (0..5).map(|s| corner(own, s, 4.0, 10.0, 24.0)).collect();
            assert!(values.windows(2).all(|p| p[0] <= p[1]), "{own}: {values:?}");
            assert_eq!(values[MEDIUM], own);
            assert_eq!(values[0], 0.0);
            assert!(values[4] <= 24.0f64.max(own));
        }
        // A square preset can still be rounded on request.
        assert_eq!(corner(0.0, 3, 4.0, 10.0, 24.0), 4.0);
        assert_eq!(corner(0.0, 4, 4.0, 10.0, 24.0), 10.0);
    }

    #[test]
    fn words_name_steps() {
        assert_eq!(step_index("xsmall"), Some(0));
        assert_eq!(step_index("medium"), Some(MEDIUM));
        assert_eq!(step_index("huge"), None);
    }
}
