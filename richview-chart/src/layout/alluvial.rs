//! Alluvial diagrams: rows as paths through several categorical dimensions (FEA-CH-75). Each
//! dimension is a column of bars, one per category in the order the rows first name them, and a
//! ribbon joins a category to the category of the next dimension that the same rows reach. The
//! rows are added up where they share the two categories and the category of the first
//! dimension, which gives a ribbon its colour; an optional value weighs a row, otherwise a row
//! counts 1. These sums are computed by RichView and a readout marks them as derived
//! (FEA-CH-42).

use super::graph::{self, Flow, FlowLink, FlowLook, FlowNode, Left};
use super::kit;
use super::types::Facets;
use super::{Ctx, Painted};
use crate::diag::Level;
use crate::geometry::Swatch;
use crate::limits;
use crate::rows::{self, Num};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

const RATIO: f64 = 16.0 / 9.0;

/// One panel's rows as paths: a category for each dimension and the weight of the row.
struct Table {
    paths: Vec<(Vec<String>, f64)>,
    failure: Option<String>,
}

/// The `dimensions` channel as field names.
fn dimensions(ctx: &Ctx) -> Vec<String> {
    ctx.block
        .raw
        .get("encoding")
        .and_then(|e| e.get("dimensions"))
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(|d| d.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

fn read(ctx: &mut Ctx, panel: usize, members: &[usize], dims: &[String], left: &mut Left) -> Table {
    let block = ctx.block;
    let data = ctx.rows;
    let value_field = block.encoding.y.clone();
    let mut table = Table {
        paths: Vec::new(),
        failure: None,
    };
    let skipped = graph::skipped(ctx, panel, members, value_field.as_deref());
    for &i in members {
        let row = &data[i];
        let cats: Vec<Option<String>> = dims.iter().map(|d| rows::text(row, d)).collect();
        if cats.iter().any(Option::is_none) {
            left.rows += 1;
            continue;
        }
        let value = match &value_field {
            None => 1.0,
            Some(field) => match rows::num(row, field) {
                Num::Value(_) if skipped.negative.contains(&i) => {
                    left.rows += 1;
                    continue;
                }
                Num::Value(_) if skipped.zero.contains(&i) => 0.0,
                // A value that is only zero on the grid still weighs something.
                Num::Value(v) => v.max(f64::MIN_POSITIVE),
                Num::Missing => {
                    left.rows += 1;
                    continue;
                }
                Num::Underflow(t) => {
                    ctx.underflow(i, field, &t);
                    table.failure.get_or_insert_with(|| {
                        format!("Cannot draw this alluvial diagram · {field} has a value too small to represent")
                    });
                    continue;
                }
                Num::Invalid(t) => {
                    ctx.check(
                        "non-numeric",
                        Level::Error,
                        format!("Row {} has {field} '{t}', which is not a number", i + 1),
                        Some("Write plain numbers; missing values may be empty or NA."),
                        Some(Ctx::row_path(i, field)),
                        None,
                        json!({"row": i}),
                    );
                    table.failure.get_or_insert_with(|| {
                        format!("Cannot draw this alluvial diagram · {field} has text that is not a number")
                    });
                    continue;
                }
            },
        };
        if skipped.zero.contains(&i) {
            left.zero += 1;
            continue;
        }
        table.paths.push((cats.into_iter().flatten().collect(), value));
    }
    if table.failure.is_none() {
        let total: f64 = table.paths.iter().map(|p| p.1).sum();
        if !total.is_finite() {
            ctx.check(
                "value-overflow",
                Level::Error,
                "The values add up to more than can be represented",
                None,
                Some("chart.encoding.y".into()),
                None,
                json!({}),
            );
            table.failure = Some("Cannot draw this alluvial diagram · The values are too large".into());
        } else if table.paths.is_empty() {
            table.failure = Some("Nothing to draw · No row has a category in every dimension".into());
        }
    }
    table
}

/// The flow of one table: a node for each category of each dimension, a ribbon for each pair of
/// neighbouring categories and category of the first dimension. `firsts` are the colour slots.
fn build(table: &Table, dims: &[String], firsts: &[String]) -> Flow {
    let mut nodes: Vec<FlowNode> = Vec::new();
    let mut ids: Vec<HashMap<String, usize>> = vec![HashMap::new(); dims.len()];
    for (cats, _) in &table.paths {
        for (k, cat) in cats.iter().enumerate() {
            if !ids[k].contains_key(cat) {
                ids[k].insert(cat.clone(), nodes.len());
                nodes.push(FlowNode {
                    name: cat.clone(),
                    column: k,
                    slot: None,
                });
            }
        }
    }
    let mut sums: BTreeMap<(usize, usize, usize, usize), f64> = BTreeMap::new();
    for (cats, value) in &table.paths {
        let colour = firsts.iter().position(|f| *f == cats[0]).unwrap_or(0);
        for (k, pair) in cats.windows(2).enumerate() {
            let (a, b) = (ids[k][&pair[0]], ids[k + 1][&pair[1]]);
            *sums.entry((k, a, b, colour)).or_insert(0.0) += value;
        }
    }
    let links = sums
        .into_iter()
        .map(|((k, source, target, colour), value)| FlowLink {
            source,
            target,
            value,
            slot: colour,
            row: None,
            group: if k > 0 { firsts.get(colour).cloned() } else { None },
        })
        .collect();
    Flow {
        nodes,
        links,
        columns: dims.len(),
        titles: dims.to_vec(),
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let dims = dimensions(ctx);
    if dims.len() < 2 {
        ctx.check(
            "source-error",
            Level::Error,
            "An alluvial diagram needs encoding.dimensions with at least two fields",
            Some("List the categorical fields, one column each."),
            Some("chart.encoding.dimensions".into()),
            None,
            Value::Null,
        );
        return kit::failed(
            ctx,
            "Cannot draw this alluvial diagram · encoding.dimensions needs two fields",
            block.style.ratio.unwrap_or(RATIO),
        );
    }
    let mut left = Left::default();
    let mut tables: Vec<Table> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        tables.push(read(ctx, fi, members, &dims, &mut left));
    }
    graph::report(
        ctx,
        &left,
        "alluvial diagram",
        "a category in every dimension or a non-negative value",
        "chart.encoding.dimensions",
    );
    // Colour slots: the categories of the first dimension across panels, in first-seen order.
    let mut firsts: Vec<String> = Vec::new();
    for t in tables.iter().filter(|t| t.failure.is_none()) {
        for (cats, _) in &t.paths {
            if !firsts.contains(&cats[0]) {
                firsts.push(cats[0].clone());
            }
        }
    }
    ctx.set_series(&firsts);
    let mut flows: Vec<Result<Flow, String>> = tables
        .iter()
        .map(|t| match &t.failure {
            Some(reason) => Err(reason.clone()),
            None => Ok(build(t, &dims, &firsts)),
        })
        .collect();
    // Above a declared limit a panel is not drawn and nothing is dropped (FEA-CH-75).
    for flow in &mut flows {
        let Ok(f) = flow else { continue };
        let mut reason = None;
        for (what, count, path) in [
            ("nodes", f.nodes.len(), "chart.encoding.dimensions"),
            ("links", f.links.len(), "chart.encoding.dimensions"),
        ] {
            if count > limits::max("alluvial", what) {
                reason.get_or_insert(kit::over_limit(ctx, "alluvial", what, count, Some(path)));
            }
        }
        if let Some(reason) = reason {
            *flow = Err(reason);
        }
    }
    let paths: usize = tables
        .iter()
        .filter(|t| t.failure.is_none())
        .map(|t| t.paths.len())
        .sum();
    ctx.summary = format!(
        "Alluvial diagram of {} across {}: {}.",
        super::count(paths, "row", "rows"),
        super::count(dims.len(), "dimension", "dimensions"),
        dims.join(", ")
    );
    ctx.note
        .push("ribbons add up the rows that share two neighbouring categories and the first category".into());
    let value_name = match &block.encoding.y {
        Some(field) => block
            .y
            .label
            .clone()
            .unwrap_or_else(|| super::cartesian::readable_name(field)),
        None => "Rows".to_owned(),
    };
    let look = FlowLook {
        ratio: block.style.ratio.unwrap_or(RATIO),
        relax: false,
        value_name,
        derived_links: true,
        share_name: "Share of the whole",
        group_name: Some(super::cartesian::readable_name(&dims[0])),
    };
    let mut painted = graph::paint_flows(ctx, facets, &flows, &look);
    if painted.failure.is_none() {
        painted.legend = kit::legend(ctx, Swatch::Square);
    }
    painted
}
