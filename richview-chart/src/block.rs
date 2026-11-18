//! A chart block as the engine reads it (TYP-12 to TYP-20, TYP-77, TYP-78).
//!
//! Core's schema registry validates every block before it reaches the engine: unknown keys are
//! dropped with `block.unknown-key` and invalid values fail with `block.schema` (TYP-10). The
//! engine therefore reads an already normalized block and only applies defaults here.

use crate::numbers::{self, Format, Kind, Locale};
use serde_json::Value;

/// A chart type the engine draws (TYP-14): an index into the table in `layout::types`, so a new
/// type is one painter and one table row. The nine constants below are the types the code
/// names; every other type is reached by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartType(pub(crate) u8);

#[allow(non_upper_case_globals)]
impl ChartType {
    pub const Line: ChartType = ChartType(0);
    pub const Area: ChartType = ChartType(1);
    pub const Bar: ChartType = ChartType(2);
    pub const Scatter: ChartType = ChartType(3);
    pub const Pie: ChartType = ChartType(4);
    pub const Heatmap: ChartType = ChartType(5);
    pub const Histogram: ChartType = ChartType(6);
    pub const Boxplot: ChartType = ChartType(7);
    pub const Interval: ChartType = ChartType(8);

    pub fn parse(name: &str) -> Option<Self> {
        crate::layout::types::DEFS
            .iter()
            .position(|d| d.name == name)
            .map(|i| ChartType(i as u8))
    }
    /// Every type, in registry order.
    pub fn all() -> impl Iterator<Item = ChartType> {
        (0..crate::layout::types::DEFS.len()).map(|i| ChartType(i as u8))
    }
    pub fn name(self) -> &'static str {
        crate::layout::types::def(self).name
    }
    /// Default aspect ratio, width over height (TYP-18).
    pub fn default_ratio(self, calendar: bool) -> f64 {
        let def = crate::layout::types::def(self);
        if calendar { def.calendar_ratio } else { def.ratio }
    }
    /// What more width gives a chart of this type.
    pub fn wider(self) -> Wider {
        crate::layout::types::def(self).wider
    }
}

/// What more width gives a chart type. Each type's row decides it, so a new type cannot leave it
/// out. Only a type that shows more turns a quarter on a phone held upright (FEA-CH-68).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wider {
    /// More of the data: an x scale of time, ordered categories or values spreads its ticks,
    /// labels and marks.
    ShowsMore,
    /// Only a larger drawing, as for flows, hierarchies, radial and area-filling charts, maps,
    /// networks, word clouds, funnels and 3D scenes.
    Enlarges,
}

/// Field names per channel (TYP-14).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Encoding {
    pub x: Option<String>,
    pub y: Option<String>,
    pub color: Option<String>,
    pub size: Option<String>,
    pub label: Option<String>,
    pub lower: Option<String>,
    pub upper: Option<String>,
    pub facet: Option<String>,
    pub detail: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Axis {
    pub label: Option<String>,
    pub format: Option<Kind>,
    pub unit: Option<String>,
    pub digits: Option<u8>,
    pub locale: Option<Locale>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Auto,
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Curve {
    Linear,
    #[default]
    Monotone,
    Step,
    StepBefore,
    StepAfter,
}

impl Curve {
    pub fn parse(name: &str) -> Option<Curve> {
        Some(match name {
            "linear" => Curve::Linear,
            "monotone" => Curve::Monotone,
            "step" => Curve::Step,
            "step_before" => Curve::StepBefore,
            "step_after" => Curve::StepAfter,
            _ => return None,
        })
    }
}

/// Which observations a line or area path retains (FEA-CH-17). Interpolation is independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Points {
    #[default]
    All,
    M4,
    Band,
}

impl Points {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "all" => Self::All,
            "m4" => Self::M4,
            "band" => Self::Band,
            _ => return None,
        })
    }
}

/// A palette named by builtin style, or authored colours (STY-15, STY-53).
#[derive(Debug, Clone, PartialEq)]
pub enum PaletteRef {
    Named(String),
    Colors(Vec<String>),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChartStyleFields {
    pub palette: Option<PaletteRef>,
    /// Width over height.
    pub ratio: Option<f64>,
    pub orientation: Orientation,
    pub curve: Option<Curve>,
    pub curvature: Option<f64>,
    /// None uses the measured automatic ceiling; explicit All has no ceiling.
    pub points: Option<Points>,
    /// `facet_scale: free` frees each panel's value scale (FEA-CH-36).
    pub free_scales: bool,
    /// `gaps: bridge` draws over missing runs (FEA-CH-38).
    pub bridge_gaps: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationKind {
    Point,
    Line,
    Text,
    Range,
    Band,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub kind: AnnotationKind,
    /// Anchor: data field or axis alias (`x`, `y`, `name`) to value.
    pub at: Vec<(String, Value)>,
    pub from: Vec<(String, Value)>,
    pub to: Vec<(String, Value)>,
    pub text: Option<String>,
    pub rotate: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Measure {
    #[default]
    Count,
    Proportion,
    Density,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Binning {
    pub count: Option<usize>,
    pub extent: Option<[f64; 2]>,
    pub edges: Option<Vec<f64>>,
    pub measure: Measure,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Highlight {
    pub by: String,
    pub include: Vec<String>,
    pub priority: Vec<String>,
}

/// A validated chart block with defaults applied.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartBlock {
    pub kind: ChartType,
    pub title: String,
    pub subtitle: Option<String>,
    pub encoding: Encoding,
    pub x: Axis,
    pub y: Axis,
    pub color: Axis,
    pub style: ChartStyleFields,
    pub annotations: Vec<Annotation>,
    pub binning: Binning,
    pub stack: bool,
    pub wide: bool,
    /// Whether automatic value labels and default point names are drawn (FEA-CH-28).
    pub auto_annotations: bool,
    pub highlight: Option<Highlight>,
    /// Significant digits for labels (TYP-23).
    pub digits: u8,
    /// Significant digits for readouts, Table and CSV; never below `digits`.
    pub readout_digits: u8,
    /// The block as validated, for the channels and settings only some types read.
    pub raw: Value,
}

fn string(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|s| !s.trim().is_empty())
}

fn number(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64).filter(|n| n.is_finite())
}

fn digits(v: &Value, key: &str) -> Option<u8> {
    v.get(key)
        .and_then(Value::as_u64)
        .map(|d| d.clamp(1, numbers::MAX_DIGITS as u64) as u8)
}

fn anchor(v: Option<&Value>) -> Vec<(String, Value)> {
    match v {
        Some(Value::Object(map)) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => Vec::new(),
    }
}

fn axis(v: &Value) -> Axis {
    Axis {
        label: string(v, "label"),
        format: v.get("format").and_then(Value::as_str).and_then(Kind::parse),
        unit: string(v, "unit"),
        digits: digits(v, "digits"),
        locale: v.get("locale").and_then(Value::as_str).map(Locale::from_tag),
        min: number(v, "min"),
        max: number(v, "max"),
    }
}

/// Parse `W:H` or one number (TYP-85).
pub fn parse_ratio(v: &Value) -> Option<f64> {
    let ratio = match v {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => match s.split_once(':') {
            Some((w, h)) => w.trim().parse::<f64>().ok()? / h.trim().parse::<f64>().ok()?,
            None => s.trim().parse().ok()?,
        },
        _ => return None,
    };
    (ratio.is_finite() && (0.5..=6.0).contains(&ratio)).then_some(ratio)
}

impl ChartBlock {
    /// Read a validated block. Returns `None` only when `type` is not a chart type, which
    /// validation has already reported.
    pub fn from_value(v: &Value) -> Option<ChartBlock> {
        let kind = ChartType::parse(v.get("type")?.as_str()?)?;
        let enc = v.get("encoding").cloned().unwrap_or(Value::Null);
        let encoding = Encoding {
            x: string(&enc, "x"),
            y: string(&enc, "y"),
            color: string(&enc, "color"),
            size: string(&enc, "size"),
            label: string(&enc, "label"),
            lower: string(&enc, "lower"),
            upper: string(&enc, "upper"),
            facet: string(&enc, "facet"),
            detail: match enc.get("detail") {
                Some(Value::Array(items)) => items.iter().filter_map(|i| i.as_str().map(str::to_owned)).collect(),
                Some(Value::String(s)) => vec![s.clone()],
                _ => Vec::new(),
            },
        };
        let axes = v.get("axes").cloned().unwrap_or(Value::Null);
        let s = v.get("style").cloned().unwrap_or(Value::Null);
        let style = ChartStyleFields {
            palette: match s.get("palette") {
                Some(Value::String(name)) => Some(PaletteRef::Named(name.clone())),
                Some(Value::Array(colors)) => Some(PaletteRef::Colors(
                    colors.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect(),
                )),
                _ => None,
            },
            ratio: s.get("ratio").and_then(parse_ratio),
            orientation: match s.get("orientation").and_then(Value::as_str) {
                Some("vertical") => Orientation::Vertical,
                Some("horizontal") => Orientation::Horizontal,
                _ => Orientation::Auto,
            },
            curve: s.get("curve").and_then(Value::as_str).and_then(Curve::parse),
            curvature: number(&s, "curvature").map(|c| c.clamp(0.0, 1.0)),
            points: s.get("points").and_then(Value::as_str).and_then(Points::parse),
            free_scales: s.get("facet_scale").and_then(Value::as_str) == Some("free"),
            bridge_gaps: s.get("gaps").and_then(Value::as_str) == Some("bridge"),
        };
        let annotations = match v.get("annotations") {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|a| {
                    let kind = match a.get("kind")?.as_str()? {
                        "point" => AnnotationKind::Point,
                        "line" => AnnotationKind::Line,
                        "text" => AnnotationKind::Text,
                        "range" => AnnotationKind::Range,
                        "band" => AnnotationKind::Band,
                        _ => return None,
                    };
                    Some(Annotation {
                        kind,
                        at: anchor(a.get("at")),
                        from: anchor(a.get("from")),
                        to: anchor(a.get("to")),
                        text: string(a, "text"),
                        rotate: if kind == AnnotationKind::Text {
                            number(a, "rotate").unwrap_or(0.0)
                        } else {
                            0.0
                        },
                    })
                })
                .collect(),
            _ => Vec::new(),
        };
        let b = v.get("binning").cloned().unwrap_or(Value::Null);
        let binning = Binning {
            count: b.get("count").and_then(Value::as_u64).map(|c| c.clamp(1, 256) as usize),
            extent: b
                .get("extent")
                .and_then(Value::as_array)
                .and_then(|e| Some([e.first()?.as_f64()?, e.get(1)?.as_f64()?])),
            edges: b
                .get("edges")
                .and_then(Value::as_array)
                .map(|e| e.iter().filter_map(Value::as_f64).collect()),
            measure: match b.get("measure").and_then(Value::as_str) {
                Some("proportion") => Measure::Proportion,
                Some("density") => Measure::Density,
                _ => Measure::Count,
            },
        };
        let highlight = v.get("highlight").and_then(|h| {
            let list = |key: &str| -> Vec<String> {
                h.get(key)
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| s.as_str().map(str::to_owned).or_else(|| Some(s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            Some(Highlight {
                by: string(h, "by")?,
                include: list("include"),
                priority: list("priority"),
            })
        });
        let chart_digits = digits(v, "digits").unwrap_or(numbers::DEFAULT_DIGITS);
        Some(ChartBlock {
            kind,
            title: string(v, "title").unwrap_or_default(),
            subtitle: string(v, "subtitle"),
            encoding,
            x: axis(axes.get("x").unwrap_or(&Value::Null)),
            y: axis(axes.get("y").unwrap_or(&Value::Null)),
            color: axis(axes.get("color").unwrap_or(&Value::Null)),
            style,
            annotations,
            binning,
            stack: v.get("stack").and_then(Value::as_bool).unwrap_or(false),
            wide: v.get("wide").and_then(Value::as_bool).unwrap_or(false),
            auto_annotations: v.get("auto_annotations").and_then(Value::as_bool).unwrap_or(true),
            highlight,
            digits: chart_digits,
            readout_digits: digits(v, "readout_digits").unwrap_or(chart_digits).max(chart_digits),
            raw: v.clone(),
        })
    }

    /// The options of the `size` axis (`axes.size`): the name, format and unit of the bubble
    /// size key. Absent options leave every field empty.
    pub fn size_axis(&self) -> Axis {
        axis(self.raw.get("axes").and_then(|a| a.get("size")).unwrap_or(&Value::Null))
    }

    /// The field an `encoding` channel names, for channels beyond the fixed ones.
    pub fn channel(&self, name: &str) -> Option<String> {
        self.raw.get("encoding").and_then(|e| string(e, name))
    }

    /// The fields a list channel names (`dimensions`), in the order written; empty when the
    /// channel is absent or not a list. Entries that are not text are skipped.
    pub fn channel_list(&self, name: &str) -> Vec<String> {
        self.raw
            .get("encoding")
            .and_then(|e| e.get(name))
            .and_then(Value::as_array)
            .map(|l| l.iter().filter_map(|f| f.as_str().map(str::to_owned)).collect())
            .unwrap_or_default()
    }

    /// A text setting under `style`.
    pub fn style_text(&self, name: &str) -> Option<String> {
        self.raw.get("style").and_then(|s| string(s, name))
    }

    /// A numeric setting under `style`.
    pub fn style_number(&self, name: &str) -> Option<f64> {
        self.raw.get("style").and_then(|s| number(s, name))
    }

    /// Label format for a value axis or channel, in the axis's own `locale` when it sets one.
    pub fn label_format(&self, axis: &Axis, locale: Locale) -> Format {
        Format {
            kind: axis.format.unwrap_or(Kind::Auto),
            unit: axis.unit.clone(),
            digits: axis.digits.unwrap_or(self.digits),
            locale: axis.locale.unwrap_or(locale),
        }
    }

    /// The significant digits an automatic value prints with on a value axis or channel: the
    /// axis's own, else the chart's (FEA-CH-37). Readouts may carry more.
    pub fn label_digits(&self, axis: &Axis) -> u8 {
        axis.digits.unwrap_or(self.digits)
    }

    /// Readout format for a value axis or channel: the author's readout precision (FEA-CH-37).
    pub fn readout_format(&self, axis: &Axis, locale: Locale) -> Format {
        Format {
            kind: axis.format.unwrap_or(Kind::Auto),
            unit: axis.unit.clone(),
            digits: self.readout_digits.max(axis.digits.unwrap_or(0)),
            locale: axis.locale.unwrap_or(locale),
        }
    }

    /// The data scale shared by `column`'s measured channels (TYP-36). Callers establish that
    /// x and colour are numeric; names, facets and detail-only fields have no position scale.
    pub fn scale_of(&self, column: &str) -> Option<String> {
        let encoding = self.raw.get("encoding")?.as_object()?;
        let channels: Vec<&str> = encoding
            .iter()
            .filter_map(|(key, value)| {
                (value.as_str() == Some(column)
                    || value
                        .as_array()
                        .is_some_and(|list| list.iter().any(|v| v.as_str() == Some(column))))
                .then_some(key.as_str())
            })
            .collect();
        for key in &channels {
            match *key {
                "y" | "lower" | "upper" | "open" | "high" | "low" | "close" | "values" => return Some("value".into()),
                "target" if self.kind.name() == "bullet" => return Some("value".into()),
                "start" | "end" => return Some("time".into()),
                "size" | "z" | "angle" | "line" => return Some((*key).into()),
                "dimensions" => return Some(format!("dimension:{column}")),
                _ => {}
            }
        }
        channels
            .into_iter()
            .find(|key| matches!(*key, "x" | "color"))
            .map(str::to_owned)
    }

    /// Whether `field` is read through a channel beyond the fixed ones (region, open, parent, ...).
    /// Those keep the digits that tell their values apart, as a position does.
    pub fn is_extra_channel(&self, field: &str) -> bool {
        const FIXED: [&str; 10] = [
            "x",
            "y",
            "color",
            "size",
            "label",
            "lower",
            "upper",
            "facet",
            "detail",
            "highlight",
        ];
        let extra = |channels: &serde_json::Map<String, Value>| {
            channels.iter().any(|(key, value)| {
                !FIXED.contains(&key.as_str())
                    && (value.as_str() == Some(field)
                        || value
                            .as_array()
                            .is_some_and(|l| l.iter().any(|f| f.as_str() == Some(field))))
            })
        };
        self.raw.get("encoding").and_then(Value::as_object).is_some_and(extra)
    }

    /// Every field the chart references: what the payload embeds (TYP-36).
    pub fn referenced_fields(&self) -> Vec<String> {
        let e = &self.encoding;
        let mut fields: Vec<String> = [&e.x, &e.y, &e.color, &e.size, &e.label, &e.lower, &e.upper, &e.facet]
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        fields.extend(e.detail.iter().cloned());
        // Channels beyond the fixed ones (region, open, source, ...) name fields too.
        if let Some(extra) = self.raw.get("encoding").and_then(Value::as_object) {
            for value in extra.values() {
                match value {
                    Value::String(f) => fields.push(f.clone()),
                    Value::Array(list) => fields.extend(list.iter().filter_map(|f| f.as_str().map(str::to_string))),
                    _ => {}
                }
            }
        }
        if let Some(h) = &self.highlight {
            fields.push(h.by.clone());
        }
        for a in &self.annotations {
            for (key, _) in a.at.iter().chain(&a.from).chain(&a.to) {
                if !matches!(key.as_str(), "x" | "y" | "name" | "value") {
                    fields.push(key.clone());
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        fields.retain(|f| seen.insert(f.clone()));
        fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_a_block_with_defaults() {
        let block = ChartBlock::from_value(&json!({
            "type": "line", "title": "CO2", "encoding": {"x": "month", "y": "ppm", "color": "site"},
            "axes": {"y": {"unit": "ppm", "min": 400}}, "style": {"ratio": "16:9", "curve": "step_after"},
            "readout_digits": 5
        }))
        .unwrap();
        assert_eq!(block.kind, ChartType::Line);
        assert_eq!(block.y.min, Some(400.0));
        assert!((block.style.ratio.unwrap() - 16.0 / 9.0).abs() < 1e-12);
        assert_eq!(block.style.curve, Some(Curve::StepAfter));
        assert_eq!((block.digits, block.readout_digits), (3, 5));
        assert_eq!(block.referenced_fields(), ["month", "ppm", "site"]);
    }

    #[test]
    fn ratios_outside_the_grammar_are_rejected() {
        assert_eq!(parse_ratio(&json!("2:1")), Some(2.0));
        assert_eq!(parse_ratio(&json!(1.5)), Some(1.5));
        assert_eq!(parse_ratio(&json!("10:1")), None);
        assert_eq!(parse_ratio(&json!("wide")), None);
    }
}
