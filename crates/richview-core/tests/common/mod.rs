//! What the report runtime draws for a rendered report: each embedded payload laid out by the
//! chart engine, as the browser does at the figure's width.
#![allow(dead_code)]

use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Geometry, Item, Role};
use richview_chart::layout::{Options, layout};
use richview_chart::payload::Payload;
use serde_json::Value;

pub fn payload(value: &Value) -> Payload {
    serde_json::from_value(value.clone()).expect("chart payload")
}

/// The geometry the runtime draws for an embedded payload at `width`.
pub fn geometry(value: &Value, width: f64) -> Geometry {
    let payload = payload(value);
    let block = ChartBlock::from_value(&payload.block).expect("chart block");
    let options = Options {
        width,
        lang: payload.lang.clone(),
        colors: payload.colors.clone(),
        ..Options::default()
    };
    let mut geometry = layout(&block, &payload.objects(), &payload.theme, &options);
    // What the runtime shows: the readouts and labels the exact rows wrote, never a number
    // formatted from the grid (TYP-36).
    for entry in payload.text.iter().filter(|t| t.state.is_empty()) {
        let observation = geometry
            .panels
            .get_mut(entry.panel)
            .and_then(|p| p.observations.get_mut(entry.obs));
        if let Some(observation) = observation {
            let readout = std::rc::Rc::make_mut(&mut observation.readout);
            readout.heading = entry.heading.clone();
            readout.lines = entry
                .lines
                .iter()
                .map(|l| richview_chart::geometry::ReadoutLine {
                    label: None,
                    name: l.name.clone(),
                    value: l.value.clone(),
                    series: l.series,
                    names_series: l.names_series,
                    derived: l.derived,
                })
                .collect();
        }
    }
    geometry.set_mark_texts(&payload.labels);
    geometry
}

/// Drawn text, optionally of one role.
pub fn texts(g: &Geometry, role: Option<Role>) -> Vec<&str> {
    g.items()
        .filter_map(|item| match item {
            Item::Text(t) if role.is_none_or(|r| t.role == r) => Some(t.text.as_str()),
            _ => None,
        })
        .collect()
}

/// Every readout value the reader can reach by pointing or stepping.
pub fn readout_values(g: &Geometry) -> Vec<&str> {
    g.panels
        .iter()
        .flat_map(|p| &p.observations)
        .flat_map(|o| o.readout.lines.iter().map(|l| l.value.as_str()))
        .collect()
}

pub fn readout_headings(g: &Geometry) -> Vec<&str> {
    g.panels
        .iter()
        .flat_map(|p| &p.observations)
        .map(|o| o.readout.heading.as_str())
        .collect()
}
