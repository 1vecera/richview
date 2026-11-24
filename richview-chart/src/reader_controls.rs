//! Reader controls on a chart (FEA-BR-01 to FEA-BR-06, TYP-77). The author declares an ordered
//! list of dimensions a reader can change: `playback` steps through a field's values, `filter`
//! keeps one value of a field, `metric` switches the measured field with its value axis, and
//! `variant` merges a partial chart onto the base. Every state resolves from the authored rows
//! alone into a complete chart: variants apply first, then the other dimensions in author order.
//! The state itself is internal to the report runtime (FEA-BR-07).

use crate::block::ChartBlock;
use crate::layout::{ChartTheme, Options, layout};
use crate::rows;
use crate::svg::escape;
use crate::time;
use serde_json::{Map, Value};

/// A state ready to lay out: the block, the rows it draws and their indices in the source rows.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub block: ChartBlock,
    pub rows: Vec<Value>,
    pub kept: Vec<usize>,
}

/// Default playback speed in milliseconds per frame (FEA-BR-02).
pub const DEFAULT_SPEED: f64 = 800.0;
/// The most states `check` resolves per chart (FEA-BR-33).
pub const STATE_LIMIT: usize = 1000;

/// How a selector shows its options (FEA-BR-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presentation {
    Buttons,
    Dropdown,
}

/// One option a reader can choose: its value as authored and the label readers see.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Frames are the field's distinct values; `None` steps through `x` (FEA-BR-02).
    Playback { field: Option<String>, speed: f64 },
    /// One value of a categorical field; with `all`, the first option keeps every row
    /// (FEA-BR-03).
    Filter { field: String, all: bool },
    /// Per option, the measured field and its value-axis fragment (FEA-BR-04).
    Metric {
        fields: Vec<String>,
        axes: Vec<Option<Map<String, Value>>>,
    },
    /// Per option, a partial chart merged key by key onto the base (FEA-BR-05).
    Variant { charts: Vec<Map<String, Value>> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    pub id: String,
    pub label: String,
    pub kind: Kind,
    /// The options in display order; a playback's frames.
    pub options: Vec<Choice>,
    pub initial: usize,
    pub presentation: Presentation,
}

impl Control {
    pub fn is_playback(&self) -> bool {
        matches!(self.kind, Kind::Playback { .. })
    }
}

/// The chosen option of each control, in author order.
pub type State = Vec<usize>;

/// A chart's controls, parsed with the frames its rows hold.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Controls {
    pub list: Vec<Control>,
}

fn text(v: &Value, key: &str) -> Option<String> {
    scalar(v.get(key)?)
}

fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn presentation(v: &Value) -> Option<Presentation> {
    match v.get("as").and_then(Value::as_str)? {
        "buttons" => Some(Presentation::Buttons),
        "dropdown" => Some(Presentation::Dropdown),
        _ => None,
    }
}

/// A playback's frames: the field's distinct values, ascending when every value is a number or
/// every value a calendar date, in first-appearance order otherwise; rows without a value belong
/// to no frame (FEA-BR-02).
pub fn frames(rows: &[Value], field: &str) -> Vec<String> {
    let values = rows::distinct(rows, field);
    let numbers: Option<Vec<f64>> = values
        .iter()
        .map(|v| match rows::parse_decimal(v) {
            Some(Ok(n)) => Some(n),
            _ => None,
        })
        .collect();
    let dates: Option<Vec<i64>> = values.iter().map(|v| time::parse_iso(v).map(|(ms, _)| ms)).collect();
    let mut keyed: Vec<(f64, String)> = match (numbers, dates) {
        (Some(n), _) => n.into_iter().zip(values).collect(),
        (None, Some(d)) => d.into_iter().map(|ms| ms as f64).zip(values).collect(),
        (None, None) => return values,
    };
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    keyed.into_iter().map(|(_, v)| v).collect()
}

/// Merge a partial chart onto `chart` key by key; `controls` is never replaced (FEA-BR-05).
fn merge(chart: &mut Value, part: &Map<String, Value>) {
    if let Some(base) = chart.as_object_mut() {
        for (k, v) in part {
            if k != "controls" {
                base.insert(k.clone(), v.clone());
            }
        }
    }
}

/// The value-axis keys a metric option's fragment carries (T-CHART-AXES).
const AXIS_KEYS: [&str; 5] = ["label", "unit", "format", "min", "max"];

impl Controls {
    /// Read `chart.controls`; `rows` give playback its frames. A dimension that cannot be read
    /// is left out here: validation reports it at its source line.
    pub fn parse(chart: &Value, rows: &[Value]) -> Controls {
        let Some(list) = chart.get("controls").and_then(Value::as_array) else {
            return Controls::default();
        };
        let x = chart.pointer("/encoding/x").and_then(Value::as_str).map(str::to_owned);
        let mut out = Vec::new();
        let mut selectors = 0;
        for (i, dim) in list.iter().enumerate() {
            let id = text(dim, "id").unwrap_or_else(|| format!("control-{}", i + 1));
            let initial_value = text(dim, "initial");
            let choices = |items: &[Value]| -> Vec<Choice> {
                items
                    .iter()
                    .filter_map(|o| match o {
                        Value::Object(_) => {
                            let value = text(o, "value")?;
                            let label = text(o, "label").unwrap_or_else(|| value.clone());
                            Some(Choice { value, label })
                        }
                        other => {
                            let value = scalar(other)?;
                            Some(Choice {
                                label: value.clone(),
                                value,
                            })
                        }
                    })
                    .collect()
            };
            let items = dim
                .get("options")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let (kind, options, default_label) = match dim.get("kind").and_then(Value::as_str) {
                Some("playback") => {
                    let field = text(dim, "field");
                    let Some(over) = field.clone().or_else(|| x.clone()) else {
                        continue;
                    };
                    let options = frames(rows, &over)
                        .into_iter()
                        .map(|v| Choice {
                            label: v.clone(),
                            value: v,
                        })
                        .collect();
                    let speed = dim
                        .get("speed")
                        .and_then(Value::as_f64)
                        .filter(|s| s.is_finite() && *s > 0.0)
                        .unwrap_or(DEFAULT_SPEED);
                    (Kind::Playback { field, speed }, options, over)
                }
                Some("filter") => {
                    let Some(field) = text(dim, "field") else {
                        continue;
                    };
                    let mut options = choices(items);
                    let all = text(dim, "all");
                    if let Some(label) = &all {
                        options.insert(
                            0,
                            Choice {
                                value: String::new(),
                                label: label.clone(),
                            },
                        );
                    }
                    (
                        Kind::Filter {
                            field: field.clone(),
                            all: all.is_some(),
                        },
                        options,
                        field,
                    )
                }
                Some("metric") => {
                    let fields = items.iter().filter_map(|o| text(o, "field")).collect::<Vec<_>>();
                    if fields.len() != items.len() {
                        continue;
                    }
                    let axes = items
                        .iter()
                        .map(|o| o.get("axis").and_then(Value::as_object).cloned())
                        .collect();
                    (Kind::Metric { fields, axes }, choices(items), "Measure".to_owned())
                }
                Some("variant") => {
                    let charts: Vec<Map<String, Value>> = items
                        .iter()
                        .filter_map(|o| o.get("chart").and_then(Value::as_object).cloned())
                        .collect();
                    if charts.len() != items.len() {
                        continue;
                    }
                    (Kind::Variant { charts }, choices(items), "View".to_owned())
                }
                _ => continue,
            };
            if options.is_empty() {
                continue;
            }
            let playback = matches!(kind, Kind::Playback { .. });
            let initial = initial_value
                .and_then(|v| options.iter().position(|o| o.value == v))
                .unwrap_or(if playback { options.len() - 1 } else { 0 });
            // Playback has its own chrome; the first selector is buttons, the rest dropdowns
            // unless `as` says otherwise (FEA-BR-06).
            let presentation = presentation(dim).unwrap_or(if playback || selectors == 0 {
                Presentation::Buttons
            } else {
                Presentation::Dropdown
            });
            if !playback {
                selectors += 1;
            }
            out.push(Control {
                id,
                label: text(dim, "label").unwrap_or(default_label),
                kind,
                options,
                initial,
                presentation,
            });
        }
        Controls { list: out }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn initial(&self) -> State {
        self.list.iter().map(|c| c.initial).collect()
    }

    /// The playback control's index, if the chart has one.
    pub fn playback(&self) -> Option<usize> {
        self.list.iter().position(Control::is_playback)
    }

    /// The chart without rows for the selector state alone: the chosen variant merged in and the
    /// chosen metric swapped in (FEA-BR-04, FEA-BR-05).
    pub fn view(&self, chart: &Value, state: &State) -> Value {
        let mut out = chart.clone();
        for (control, &chosen) in self.list.iter().zip(state) {
            if let Kind::Variant { charts } = &control.kind
                && let Some(part) = charts.get(chosen)
            {
                merge(&mut out, part);
            }
        }
        for (control, &chosen) in self.list.iter().zip(state) {
            if let Kind::Metric { fields, axes } = &control.kind
                && let Some(field) = fields.get(chosen)
            {
                if !out.get("encoding").is_some_and(Value::is_object) {
                    out["encoding"] = Value::Object(Map::new());
                }
                out["encoding"]["y"] = Value::String(field.clone());
                if !out.get("axes").is_some_and(Value::is_object) {
                    out["axes"] = Value::Object(Map::new());
                }
                if !out["axes"].get("y").is_some_and(Value::is_object) {
                    out["axes"]["y"] = Value::Object(Map::new());
                }
                // The whole fragment travels with the option: plotted values, ticks, axis
                // title and readouts change together.
                let y = out["axes"]["y"].as_object_mut().expect("axis object");
                for key in AXIS_KEYS {
                    y.remove(key);
                }
                if let Some(Some(fragment)) = axes.get(chosen) {
                    for (k, v) in fragment {
                        if AXIS_KEYS.contains(&k.as_str()) {
                            y.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
        }
        out
    }

    /// Every distinct view the selectors can produce: each variant with each metric, for core to
    /// embed the fields and precision every state needs.
    pub fn views(&self, chart: &Value) -> Vec<Value> {
        let shaping: Vec<usize> = self
            .list
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c.kind, Kind::Metric { .. } | Kind::Variant { .. }))
            .map(|(i, _)| i)
            .collect();
        let mut state = self.initial();
        let mut out = Vec::new();
        let counts: Vec<usize> = shaping.iter().map(|&i| self.list[i].options.len()).collect();
        let total: usize = counts.iter().product();
        for n in 0..total.min(STATE_LIMIT) {
            let mut rest = n;
            for (k, &i) in shaping.iter().enumerate() {
                state[i] = rest % counts[k];
                rest /= counts[k];
            }
            out.push(self.view(chart, &state));
        }
        out
    }

    /// The fields the controls themselves read: playback and filter fields and every metric's
    /// field.
    pub fn fields(&self, chart: &Value) -> Vec<String> {
        let mut out = Vec::new();
        for c in &self.list {
            match &c.kind {
                Kind::Playback { field, .. } => {
                    if let Some(f) = field
                        .clone()
                        .or_else(|| chart.pointer("/encoding/x").and_then(Value::as_str).map(str::to_owned))
                    {
                        out.push(f);
                    }
                }
                Kind::Filter { field, .. } => out.push(field.clone()),
                Kind::Metric { fields, .. } => out.extend(fields.iter().cloned()),
                Kind::Variant { .. } => {}
            }
        }
        out
    }

    /// Resolve a state: the chart to lay out and the indices of the rows it shows. With
    /// `all_frames`, playback keeps every frame, which is what the axes fit to (FEA-BR-31).
    pub fn resolve(&self, chart: &Value, rows: &[Value], state: &State, all_frames: bool) -> (Value, Vec<usize>) {
        let view = self.view(chart, state);
        let mut keep: Vec<usize> = (0..rows.len()).collect();
        let kind = view.get("type").and_then(Value::as_str).unwrap_or_default();
        let x = view.pointer("/encoding/x").and_then(Value::as_str);
        for (control, &chosen) in self.list.iter().zip(state) {
            match &control.kind {
                Kind::Filter { field, all } => {
                    if *all && chosen == 0 {
                        continue;
                    }
                    let Some(option) = control.options.get(chosen) else {
                        continue;
                    };
                    keep.retain(|&i| rows::text(&rows[i], field).as_deref() == Some(option.value.as_str()));
                }
                Kind::Playback { field, .. } => {
                    let Some(over) = field.as_deref().or(x) else {
                        continue;
                    };
                    let index = |row: &Value| {
                        rows::text(row, over).and_then(|t| control.options.iter().position(|o| o.value == t))
                    };
                    if all_frames {
                        keep.retain(|&i| index(&rows[i]).is_some());
                        continue;
                    }
                    // Lines and areas over their own x reveal the series up to the frame; other
                    // charts show the frame's rows (FEA-BR-02).
                    let reveal = matches!(kind, "line" | "area") && Some(over) == x;
                    keep.retain(|&i| match index(&rows[i]) {
                        Some(k) if reveal => k <= chosen,
                        Some(k) => k == chosen,
                        None => false,
                    });
                }
                Kind::Metric { .. } | Kind::Variant { .. } => {}
            }
        }
        (view, keep)
    }

    /// The state as the engine draws it; `None` when the chosen view is not a chart.
    pub fn chart(&self, chart: &Value, rows: &[Value], state: &State) -> Option<Resolved> {
        let (view, kept) = self.resolve(chart, rows, state, false);
        let block = ChartBlock::from_value(&view)?;
        Some(Resolved {
            block,
            rows: kept.iter().map(|&i| rows[i].clone()).collect(),
            kept,
        })
    }

    /// Options for laying out `state`: series keep their palette slots from every row of the
    /// chosen view (FEA-BR-03), and a playback fits its scales to every frame under the other
    /// controls (FEA-BR-31). The result depends only on the selectors, so a host computes it
    /// once per selector change, not per frame.
    pub fn options(&self, chart: &Value, rows: &[Value], state: &State, theme: &ChartTheme, base: &Options) -> Options {
        let mut opts = base.clone();
        let view = self.view(chart, state);
        let Some(block) = ChartBlock::from_value(&view) else {
            return opts;
        };
        let everything = layout(&block, rows, theme, base);
        for s in &everything.series {
            if !opts.colors.iter().any(|(k, _)| *k == s.key) {
                opts.colors.push((s.key.clone(), s.slot + 1));
            }
        }
        if let Some(k) = self.playback() {
            let (_, frames) = self.resolve(chart, rows, state, true);
            let frames: Vec<Value> = frames.iter().map(|&i| rows[i].clone()).collect();
            let mut fit = layout(&block, &frames, theme, &opts).fit;
            fit.names.clear();
            opts.fit = Some(fit);
            // Every frame names the points the opening frame names, so a name moves with its
            // point through the playback instead of blinking between points (FEA-CH-28).
            let mut opening = state.clone();
            if let (Some(frame), Some(c)) = (opening.get_mut(k), self.list.get(k)) {
                *frame = c.initial;
            }
            let (_, shown) = self.resolve(chart, rows, &opening, false);
            let shown: Vec<Value> = shown.iter().map(|&i| rows[i].clone()).collect();
            let names = layout(&block, &shown, theme, &opts).fit.names;
            if let Some(fit) = &mut opts.fit {
                fit.names = names;
            }
        }
        opts
    }

    /// The selector state of `state`: its playback at the first frame, a cache key for
    /// [`Controls::options`].
    pub fn selectors(&self, state: &State) -> State {
        self.list
            .iter()
            .zip(state)
            .map(|(c, &i)| if c.is_playback() { 0 } else { i })
            .collect()
    }

    /// Every reachable state, at most `limit`, and how many exist (FEA-BR-33).
    pub fn states(&self, limit: usize) -> (Vec<State>, usize) {
        let counts: Vec<usize> = self.list.iter().map(|c| c.options.len()).collect();
        let total = counts
            .iter()
            .try_fold(1usize, |n, &c| n.checked_mul(c))
            .unwrap_or(usize::MAX);
        let mut out = Vec::new();
        for n in 0..total.min(limit) {
            let mut rest = n;
            out.push(
                counts
                    .iter()
                    .map(|&c| {
                        let v = rest % c;
                        rest /= c;
                        v
                    })
                    .collect(),
            );
        }
        (out, total)
    }

    /// The state in a few words for titles: the chosen options, e.g. "2000, Europe"
    /// (FEA-BR-32).
    pub fn name(&self, state: &State) -> String {
        self.list
            .iter()
            .zip(state)
            .filter_map(|(c, &i)| c.options.get(i).map(|o| o.label.as_str()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The state with each control named, for diagnostics: "Year: 2000 · View: Bars"
    /// (FEA-BR-33).
    pub fn describe(&self, state: &State) -> String {
        self.list
            .iter()
            .zip(state)
            .filter_map(|(c, &i)| c.options.get(i).map(|o| format!("{}: {}", c.label, o.label)))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// The control row above the plot (FEA-BR-06, FEA-BR-34). Core reserves it with the initial
    /// state, inert until the runtime wires it, so mounting moves nothing (FEA-CH-64).
    pub fn html(&self, state: &State, id: &str) -> String {
        let mut out = String::from(
            "<div class=\"rv-chart__controls\" role=\"group\" aria-label=\"Chart controls\" data-rv-pending inert>",
        );
        for (k, (c, &chosen)) in self.list.iter().zip(state).enumerate() {
            let label_id = format!("{id}-control-{k}");
            let label = escape(&c.label);
            let current = c.options.get(chosen).map(|o| escape(&o.label)).unwrap_or_default();
            if c.is_playback() {
                let width = c.options.iter().map(|o| o.label.chars().count()).max().unwrap_or(1);
                out.push_str(&format!(
                    "<div class=\"rv-chart__control rv-chart__playback\" role=\"group\" aria-labelledby=\"{label_id}\" data-rv-control=\"{k}\"><span class=\"rv-chart__control-label\" id=\"{label_id}\">{label}</span>{}{}{}<input type=\"range\" class=\"rv-chart__scrubber\" min=\"0\" max=\"{}\" step=\"1\" value=\"{chosen}\" aria-labelledby=\"{label_id}\" aria-valuetext=\"{current}\"><output class=\"rv-chart__frame-value\" aria-live=\"polite\" style=\"min-width:{width}ch\">{current}</output></div>",
                    control_button("previous", &format!("Previous {}", c.label), false),
                    control_button("play", "Play", true),
                    control_button("next", &format!("Next {}", c.label), false),
                    c.options.len().saturating_sub(1),
                ));
                continue;
            }
            match c.presentation {
                Presentation::Buttons => {
                    out.push_str(&format!(
                        "<div class=\"rv-chart__control\" role=\"group\" aria-labelledby=\"{label_id}\" data-rv-control=\"{k}\"><span class=\"rv-chart__control-label\" id=\"{label_id}\">{label}</span>"
                    ));
                    for (i, o) in c.options.iter().enumerate() {
                        out.push_str(&format!(
                            "<button type=\"button\" class=\"rv-chart__option\" data-rv-option=\"{i}\" aria-pressed=\"{}\">{}</button>",
                            i == chosen,
                            escape(&o.label)
                        ));
                    }
                    out.push_str("</div>");
                }
                Presentation::Dropdown => {
                    out.push_str(&format!(
                        "<label class=\"rv-chart__control\" data-rv-control=\"{k}\"><span class=\"rv-chart__control-label\">{label}</span><select class=\"rv-chart__select\">"
                    ));
                    for (i, o) in c.options.iter().enumerate() {
                        out.push_str(&format!(
                            "<option value=\"{i}\"{}>{}</option>",
                            if i == chosen { " selected" } else { "" },
                            escape(&o.label)
                        ));
                    }
                    out.push_str("</select></label>");
                }
            }
        }
        out.push_str("</div>");
        out
    }
}

/// A 16-unit icon for a playback button; play carries the pause shape too, shown while playing.
fn control_button(kind: &str, label: &str, toggle: bool) -> String {
    let path = match kind {
        "previous" => "M10 3.5 5.5 8l4.5 4.5",
        "next" => "M6 3.5 10.5 8 6 12.5",
        _ => "M5 3v10l8-5z",
    };
    let pause = if toggle {
        "<path class=\"rv-chart__pause\" d=\"M5 3h2v10H5zM9 3h2v10H9z\" fill=\"currentColor\" stroke=\"none\"/>"
    } else {
        ""
    };
    let fill = if kind == "play" {
        " class=\"rv-chart__play-shape\" fill=\"currentColor\" stroke=\"none\""
    } else {
        ""
    };
    format!(
        "<button type=\"button\" class=\"rv-chart__control-button rv-chart__control-button--{kind}\" aria-label=\"{}\"{}><svg viewBox=\"0 0 16 16\" aria-hidden=\"true\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"{path}\"{fill}/>{pause}</svg></button>",
        escape(label),
        if toggle { " aria-pressed=\"false\"" } else { "" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn gapminder() -> (Value, Vec<Value>) {
        let chart = json!({
            "type": "scatter",
            "encoding": {"x": "income", "y": "life", "size": "pop", "color": "region"},
            "controls": [
                {"kind": "playback", "id": "year", "field": "year", "label": "Year", "initial": "2000"},
                {"kind": "filter", "id": "region", "field": "region", "label": "Region",
                 "options": ["Europe", "Asia"], "all": "World"}
            ]
        });
        let rows = vec![
            json!({"year": "2000", "region": "Europe", "income": 22000, "life": 78, "pop": 5}),
            json!({"year": "1960", "region": "Europe", "income": 8000, "life": 70, "pop": 4}),
            json!({"year": "1960", "region": "Asia", "income": 2000, "life": 55, "pop": 17}),
            json!({"year": "2000", "region": "Asia", "income": 9000, "life": 72, "pop": 37}),
            json!({"year": null, "region": "Asia", "income": 1, "life": 1, "pop": 1}),
        ];
        (chart, rows)
    }

    #[test]
    fn frames_sort_numbers_and_dates_and_keep_text_in_source_order() {
        let rows = |vals: &[&str]| vals.iter().map(|v| json!({"f": v})).collect::<Vec<_>>();
        assert_eq!(
            frames(&rows(&["2000", "1960", "1980", "1960"]), "f"),
            ["1960", "1980", "2000"]
        );
        assert_eq!(
            frames(&rows(&["2020-04-01", "2020-01-01"]), "f"),
            ["2020-01-01", "2020-04-01"]
        );
        assert_eq!(frames(&rows(&["late", "early", "late"]), "f"), ["late", "early"]);
        let mut with_gap = rows(&["2", "1"]);
        with_gap.push(json!({"f": null}));
        assert_eq!(
            frames(&with_gap, "f"),
            ["1", "2"],
            "a row without a value is in no frame"
        );
    }

    #[test]
    fn a_state_is_the_frame_and_the_filter_together() {
        let (chart, rows) = gapminder();
        let c = Controls::parse(&chart, &rows);
        assert_eq!(c.list[0].options.len(), 2);
        assert_eq!(c.list[1].options[0].label, "World", "the all option leads");
        let initial = c.initial();
        assert_eq!(initial, [1, 0], "the authored frame, the first option");
        assert_eq!(c.resolve(&chart, &rows, &initial, false).1, [0, 3]);
        assert_eq!(c.resolve(&chart, &rows, &vec![1, 2], false).1, [3]);
        assert_eq!(c.resolve(&chart, &rows, &vec![0, 1], false).1, [1]);
        // Axes fit every frame under the current filter, never the row without a frame.
        assert_eq!(c.resolve(&chart, &rows, &vec![0, 2], true).1, [2, 3]);
        assert_eq!(c.name(&vec![1, 2]), "2000, Asia");
        assert_eq!(c.describe(&vec![1, 2]), "Year: 2000 · Region: Asia");
    }

    #[test]
    fn lines_over_their_own_x_reveal_up_to_the_frame() {
        let chart = json!({"type": "line", "encoding": {"x": "q", "y": "v"},
            "controls": [{"kind": "playback", "id": "q"}]});
        let rows: Vec<Value> = ["2020-01-01", "2020-04-01", "2020-07-01"]
            .iter()
            .map(|q| json!({"q": q, "v": 1}))
            .collect();
        let c = Controls::parse(&chart, &rows);
        assert_eq!(c.initial(), [2], "without `initial` the last frame");
        assert_eq!(c.resolve(&chart, &rows, &vec![1], false).1, [0, 1]);
        assert_eq!(c.list[0].label, "q");
    }

    #[test]
    fn a_metric_swaps_the_field_and_the_whole_axis_fragment() {
        let chart = json!({"type": "line", "encoding": {"x": "q", "y": "abs"},
        "axes": {"y": {"label": "Indicator", "unit": "units", "min": 0}},
        "controls": [{"kind": "metric", "id": "m", "options": [
            {"value": "abs", "label": "Absolute", "field": "abs", "axis": {"label": "Indicator", "unit": "units"}},
            {"value": "idx", "label": "Index", "field": "idx", "axis": {"label": "Indexed", "format": "number"}}
        ]}]});
        let c = Controls::parse(&chart, &[]);
        let view = c.view(&chart, &vec![1]);
        assert_eq!(view["encoding"]["y"], "idx");
        assert_eq!(view["axes"]["y"], json!({"label": "Indexed", "format": "number"}));
        assert_eq!(c.fields(&chart), ["abs", "idx"]);
        assert_eq!(c.views(&chart).len(), 2);
    }

    #[test]
    fn a_variant_merges_first_and_never_replaces_controls() {
        let chart = json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"},
        "controls": [
            {"kind": "variant", "id": "view", "options": [
                {"value": "bars", "label": "Bars", "chart": {}},
                {"value": "pie", "label": "Share", "chart": {"type": "pie", "controls": []}}
            ]},
            {"kind": "filter", "id": "g", "field": "g", "options": ["a", "b"]}
        ]});
        let c = Controls::parse(&chart, &[]);
        assert_eq!(c.list[0].presentation, Presentation::Buttons);
        assert_eq!(c.list[1].presentation, Presentation::Dropdown, "the second selector");
        let view = c.view(&chart, &vec![1, 0]);
        assert_eq!(view["type"], "pie");
        assert_eq!(view["controls"].as_array().unwrap().len(), 2);
        let (states, total) = c.states(STATE_LIMIT);
        assert_eq!((states.len(), total), (4, 4));
        assert_eq!(c.states(3).0.len(), 3);
    }

    #[test]
    fn every_frame_names_the_points_its_opening_frame_names() {
        let chart = json!({"type": "scatter", "encoding": {"x": "x", "y": "y", "label": "name"},
            "controls": [{"kind": "playback", "id": "year", "field": "year", "label": "Year"}]});
        // Each frame's extremes are other countries: A to D in 2000, E to H in 1960.
        let at = [
            ("A", [1, 5], [5, 5]),
            ("B", [9, 5], [5, 6]),
            ("C", [5, 9], [6, 5]),
            ("D", [5, 1], [4, 5]),
            ("E", [4, 4], [1, 5]),
            ("F", [6, 6], [9, 5]),
            ("G", [4, 6], [5, 9]),
            ("H", [6, 4], [5, 1]),
        ];
        let rows: Vec<Value> = at
            .iter()
            .flat_map(|(name, late, early)| {
                [
                    json!({"year": "2000", "name": name, "x": late[0], "y": late[1]}),
                    json!({"year": "1960", "name": name, "x": early[0], "y": early[1]}),
                ]
            })
            .collect();
        let c = Controls::parse(&chart, &rows);
        let theme = ChartTheme::default();
        let base = Options {
            width: 640.0,
            ..Options::default()
        };
        let opening = c.initial();
        let opts = c.options(&chart, &rows, &opening, &theme, &base);
        let kept = opts.fit.as_ref().map(|f| f.names.clone()).unwrap_or_default();
        assert!(
            ["A", "B", "C", "D"].iter().all(|n| kept.iter().any(|k| k == n)),
            "{kept:?}"
        );
        let names = |frame: usize| -> Vec<String> {
            let r = c.chart(&chart, &rows, &vec![frame]).unwrap();
            layout(&r.block, &r.rows, &theme, &opts)
                .items()
                .filter_map(|i| match i {
                    crate::geometry::Item::Text(t) if t.role == crate::geometry::Role::Label => Some(t.text.clone()),
                    _ => None,
                })
                .collect()
        };
        let (early, late) = (names(0), names(1));
        assert!(
            !early.is_empty() && early.iter().all(|n| kept.contains(n)),
            "1960 keeps the names: {early:?}"
        );
        assert!(late.iter().all(|n| kept.contains(n)), "{late:?}");
    }

    #[test]
    fn frames_share_axes_sizes_and_colours() {
        let (chart, rows) = gapminder();
        let c = Controls::parse(&chart, &rows);
        let theme = ChartTheme::default();
        let base = Options {
            width: 640.0,
            ..Options::default()
        };
        let state = vec![0, 0];
        let opts = c.options(&chart, &rows, &state, &theme, &base);
        let fit = opts.fit.clone().expect("a playback fits");
        assert_eq!(fit.x, Some([2000.0, 22000.0]), "income over both frames");
        assert_eq!(fit.size, Some(37.0));
        assert!(
            fit.size_min.is_some_and(|m| m > 0.0 && m <= 37.0),
            "the key's low end spans every frame"
        );
        let at = |frame: usize| {
            let r = c.chart(&chart, &rows, &vec![frame, 0]).unwrap();
            let g = layout(&r.block, &r.rows, &theme, &opts);
            let ticks: Vec<String> = g
                .items()
                .filter_map(|i| match i {
                    crate::geometry::Item::Text(t) if t.role == crate::geometry::Role::AxisLabel => {
                        Some(t.text.clone())
                    }
                    _ => None,
                })
                .collect();
            let colours: Vec<(String, usize)> = g.series.iter().map(|s| (s.key.clone(), s.slot)).collect();
            (ticks, colours, g.panels[0].x.as_ref().map(|s| s.domain))
        };
        let (a, b) = (at(0), at(1));
        assert_eq!(a.0, b.0, "every frame has the same ticks");
        assert_eq!(a.2, b.2, "and the same x domain");
        assert_eq!(a.1, b.1, "and the same colours");
        // A filter state keeps each region's colour from the whole data.
        let asia = c.options(&chart, &rows, &vec![1, 2], &theme, &base);
        assert!(asia.colors.contains(&("Asia".to_owned(), 2)));
    }

    #[test]
    fn the_reserved_row_is_inert_and_names_each_control() {
        let (chart, rows) = gapminder();
        let c = Controls::parse(&chart, &rows);
        let html = c.html(&c.initial(), "rv-chart-0");
        assert!(html.contains("data-rv-pending inert"));
        assert!(html.contains("aria-label=\"Previous Year\""));
        assert!(html.contains("aria-valuetext=\"2000\""));
        assert!(html.contains("<button type=\"button\" class=\"rv-chart__option\" data-rv-option=\"0\" aria-pressed=\"true\">World</button>"));
        assert!(
            html.contains("style=\"min-width:4ch\""),
            "the frame value keeps its width"
        );
    }
}
