//! Hierarchies for the charts that nest parts inside parts (treemap, sunburst, icicle, tree): rows
//! with a name, a parent's name and a value become a checked tree. Leaves carry the values the
//! author gave; a parent's value is the sum of its leaves and is shown as derived (FEA-CH-42,
//! FEA-CH-70).

use super::graph;
use super::{Ctx, Painted};
use crate::color::Oklch;
use crate::diag::Level;
use crate::geometry::{Readout, ReadoutLine, Rect};
use crate::numbers::{self, Format};
use crate::rows::{self, Num};
use serde_json::json;

pub(super) struct Node {
    pub name: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// The author's value for a leaf, `None` for a parent.
    pub given: Option<f64>,
    pub row: Option<usize>,
    pub total: f64,
    /// Where the painter placed the part.
    pub rect: Rect,
    pub depth: usize,
}

/// The tree of one panel, or why it cannot be drawn.
pub(super) struct Tree {
    pub nodes: Vec<Node>,
    pub failure: Option<String>,
    /// Leaves with no value, which a sized chart cannot draw. Zero for a tree that failed and
    /// for charts whose value is only context.
    pub excluded: usize,
}

/// Read the rows of facet `panel` (`name`, `parent` and value channels are the block's `label`,
/// `parent` and `y`). `what` names the chart in failure reasons, such as `treemap`.
pub(super) fn read(ctx: &mut Ctx, panel: usize, members: &[usize], what: &str) -> Tree {
    read_with(ctx, panel, members, what, true)
}

/// Like [`read`]. With `areas` false the value is optional context rather than a size: it may be
/// absent or negative, and a parent keeps no sum (the tree chart).
pub(super) fn read_with(ctx: &mut Ctx, panel: usize, members: &[usize], what: &str, areas: bool) -> Tree {
    let block = ctx.block;
    let name_field = block.encoding.label.clone().unwrap_or_default();
    let parent_field = block.channel("parent").unwrap_or_default();
    let value_field = block.encoding.y.clone().unwrap_or_default();
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failure: Option<String> = None;
    let mut nodes: Vec<Node> = Vec::new();
    let mut parents: Vec<Option<String>> = Vec::new();
    // Which values are zero or negative is decided by the exact rows: a small part is zero on the
    // grid but still has an area (TYP-36).
    let skipped = graph::skipped(ctx, panel, members, areas.then_some(value_field.as_str()));
    for &i in members {
        let row = &ctx.rows[i];
        let Some(name) = rows::text(row, &name_field) else {
            ctx.check(
                "excluded-observations",
                Level::Warning,
                format!("Row {} has no {name_field} and is left out", i + 1),
                None,
                Some(Ctx::row_path(i, &name_field)),
                None,
                json!({"row": i}),
            );
            continue;
        };
        if nodes.iter().any(|n| n.name == name) {
            ctx.check(
                "source-error",
                Level::Error,
                format!(
                    "Row {} repeats the name '{name}'; every part needs a unique name",
                    i + 1
                ),
                Some("Give each part its own name."),
                Some(Ctx::row_path(i, &name_field)),
                None,
                json!({"row": i}),
            );
            failure.get_or_insert_with(|| format!("Cannot draw this {what} · A name repeats"));
            continue;
        }
        let given = match rows::num(row, &value_field) {
            Num::Value(_) if areas && skipped.negative.contains(&i) => {
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!("Row {} has a negative value; an area cannot be negative", i + 1),
                    Some("Use a bar chart for signed values."),
                    Some(Ctx::row_path(i, &value_field)),
                    None,
                    json!({"row": i}),
                );
                failure.get_or_insert_with(|| format!("Cannot draw this {what} · A value is negative"));
                continue;
            }
            Num::Value(_) if areas && skipped.zero.contains(&i) => Some(0.0),
            Num::Value(v) if areas => Some(v.max(f64::MIN_POSITIVE)),
            Num::Value(v) => Some(v),
            Num::Missing => None,
            Num::Underflow(t) => {
                ctx.underflow(i, &value_field, &t);
                failure.get_or_insert_with(|| {
                    format!("Cannot draw this {what} · {value_field} has a value too small to represent")
                });
                continue;
            }
            Num::Invalid(t) => {
                ctx.check(
                    "non-numeric",
                    Level::Error,
                    format!("Row {} has {value_field} '{t}', which is not a number", i + 1),
                    None,
                    Some(Ctx::row_path(i, &value_field)),
                    None,
                    json!({"row": i}),
                );
                failure.get_or_insert_with(|| {
                    format!("Cannot draw this {what} · {value_field} has text that is not a number")
                });
                continue;
            }
        };
        parents.push(rows::text(row, &parent_field));
        nodes.push(Node {
            name,
            parent: None,
            children: Vec::new(),
            given,
            row: Some(i),
            total: 0.0,
            rect: Rect::default(),
            depth: 0,
        });
    }
    // Link children to parents by name; an unknown parent is an error, never a new root.
    for (k, parent) in parents.iter().enumerate() {
        let Some(parent) = parent else { continue };
        match nodes.iter().position(|n| &n.name == parent) {
            Some(p) if p != k => {
                nodes[k].parent = Some(p);
                nodes[p].children.push(k);
            }
            _ => {
                let row = nodes[k].row.unwrap_or(0);
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!(
                        "Row {} names the parent '{parent}', which is not a part in the data",
                        row + 1
                    ),
                    Some("Name an existing part as the parent, or leave it empty for a top-level part."),
                    Some(Ctx::row_path(row, &parent_field)),
                    None,
                    json!({"row": row, "parent": parent}),
                );
                failure.get_or_insert_with(|| format!("Cannot draw this {what} · A parent is not in the data"));
            }
        }
    }
    if failure.is_none() {
        // A cycle never reaches a root: totals would never settle.
        let mut cyclic = false;
        for k in 0..nodes.len() {
            let mut at = k;
            let mut steps = 0;
            while let Some(p) = nodes[at].parent {
                at = p;
                steps += 1;
                if steps > nodes.len() {
                    cyclic = true;
                    break;
                }
            }
        }
        if cyclic {
            ctx.check(
                "source-error",
                Level::Error,
                "The parents form a cycle",
                Some("Every part must reach a top-level part."),
                Some("chart.encoding.parent".into()),
                None,
                serde_json::Value::Null,
            );
            failure.get_or_insert_with(|| format!("Cannot draw this {what} · The parents form a cycle"));
        }
    }
    if failure.is_none() {
        if areas {
            totals(ctx, &mut nodes, &readout);
        } else {
            depths(&mut nodes);
        }
    }
    let excluded = if failure.is_none() && areas {
        nodes
            .iter()
            .filter(|n| n.children.is_empty() && n.given.is_none())
            .count()
    } else {
        0
    };
    Tree {
        nodes,
        failure,
        excluded,
    }
}

/// Say that `count` leaves without a value are left out (a check and a method note); does
/// nothing for zero.
pub(super) fn report_excluded(ctx: &mut Ctx, count: usize) {
    if count == 0 {
        return;
    }
    ctx.check(
        "excluded-observations",
        Level::Warning,
        format!(
            "{} without a value {} left out",
            super::count(count, "part", "parts"),
            super::verb(count, "is", "are")
        ),
        Some("Give every part a value."),
        Some("chart.encoding.y".into()),
        None,
        json!({"count": count}),
    );
    ctx.note.push(format!(
        "{} without a value {} not shown",
        super::count(count, "part", "parts"),
        super::verb(count, "is", "are")
    ));
}

/// The name of the share a readout gives: of everything, or of what is left after a reader
/// hid a group.
pub(super) fn share_name(ctx: &Ctx, roots: &[String]) -> &'static str {
    if (0..roots.len()).any(|slot| ctx.is_hidden(slot)) {
        "Share of the visible"
    } else {
        "Share of the whole"
    }
}

/// Depths only: each part's total is the value the author gave, or 0.
fn depths(nodes: &mut [Node]) {
    fn walk(nodes: &mut [Node], k: usize, depth: usize) {
        nodes[k].depth = depth;
        nodes[k].total = nodes[k].given.unwrap_or(0.0);
        for c in nodes[k].children.clone() {
            walk(nodes, c, depth + 1);
        }
    }
    let roots: Vec<usize> = (0..nodes.len()).filter(|&k| nodes[k].parent.is_none()).collect();
    for r in roots {
        walk(nodes, r, 0);
    }
}

/// Fill in every node's total: a leaf is its value, a parent the sum of its children. A parent
/// that also gives a value which differs from that sum is reported and keeps the sum.
fn totals(ctx: &mut Ctx, nodes: &mut [Node], readout: &Format) {
    fn sum(nodes: &mut [Node], k: usize, depth: usize) -> f64 {
        nodes[k].depth = depth;
        let children = nodes[k].children.clone();
        let total = if children.is_empty() {
            nodes[k].given.unwrap_or(0.0)
        } else {
            children.iter().map(|&c| sum(nodes, c, depth + 1)).sum()
        };
        nodes[k].total = total;
        total
    }
    let roots: Vec<usize> = (0..nodes.len()).filter(|&k| nodes[k].parent.is_none()).collect();
    for r in roots {
        sum(nodes, r, 0);
    }
    for n in nodes.iter() {
        if let (Some(g), false) = (n.given, n.children.is_empty())
            && (g - n.total).abs() > 1e-9 * g.abs().max(1.0)
        {
            let row = n.row.unwrap_or(0);
            ctx.check(
                "hierarchy-total",
                Level::Warning,
                format!(
                    "'{}' gives {} but its parts add up to {}",
                    n.name,
                    numbers::value(g, readout),
                    numbers::value(n.total, readout)
                ),
                Some("Leave the value empty on a part that has children, or make it match."),
                Some(Ctx::row_path(row, "value")),
                None,
                json!({"row": row}),
            );
        }
    }
}

pub(super) fn ancestry(nodes: &[Node], k: usize) -> Vec<String> {
    let mut path = vec![nodes[k].name.clone()];
    let mut at = k;
    while let Some(p) = nodes[at].parent {
        path.push(nodes[p].name.clone());
        at = p;
    }
    path.reverse();
    path
}

/// Top-level part names across panels in first-seen order: the colour slots.
pub(super) fn root_names(trees: &[Vec<Node>]) -> Vec<String> {
    let mut roots: Vec<String> = Vec::new();
    for tree in trees {
        for n in tree.iter().filter(|n| n.parent.is_none()) {
            if !roots.contains(&n.name) {
                roots.push(n.name.clone());
            }
        }
    }
    roots
}

/// The colour slot of the top-level part that `k` sits under.
pub(super) fn root_slot(nodes: &[Node], roots: &[String], k: usize) -> usize {
    let mut at = k;
    while let Some(p) = nodes[at].parent {
        at = p;
    }
    roots.iter().position(|r| *r == nodes[at].name).unwrap_or(0)
}

/// Fill opacity of a part by depth: deeper parts are lighter.
pub(super) fn depth_alpha(depth: usize) -> f64 {
    0.9 - 0.12 * (depth.min(4) as f64)
}

/// Whichever of ink and surface reads better on `base` drawn at `alpha` over the surface.
pub(super) fn ink_on(ctx: &Ctx, base: Oklch, alpha: f64) -> Oklch {
    let under = crate::color::mix(ctx.style.surface, base, alpha);
    if crate::color::contrast(ctx.style.ink, under) >= crate::color::contrast(ctx.style.surface, under) {
        ctx.style.ink
    } else {
        ctx.style.surface
    }
}

/// The value and the share of every part as the readout writes them, from the exact values: a
/// parent's sum and every share are numbers RichView computed (TYP-36).
pub(super) fn part_texts(ctx: &Ctx, panel: usize, nodes: &[Node], whole: f64, format: &Format) -> Vec<[String; 2]> {
    ctx.derive_visible(panel, "hierarchy.texts", || {
        nodes
            .iter()
            .map(|n| {
                [
                    numbers::value(n.total, format),
                    super::pie::share_text(n.total / whole, ctx.locale),
                ]
            })
            .collect()
    })
}

/// The readout of a part that has a size: its path, its value (derived for a parent) and its
/// share of what is drawn, written as `texts` has them.
pub(super) fn part_readout(
    nodes: &[Node],
    k: usize,
    slot: usize,
    texts: &[[String; 2]],
    value_field: &str,
    share: &str,
) -> Readout {
    let n = &nodes[k];
    let [value, part] = &texts[k];
    Readout {
        heading: ancestry(nodes, k).join(" › "),
        lines: vec![
            ReadoutLine {
                name: value_field.to_owned(),
                value: value.clone(),
                series: Some(slot),
                names_series: false,
                derived: !n.children.is_empty(),
            },
            ReadoutLine {
                name: share.to_owned(),
                value: part.clone(),
                series: None,
                names_series: false,
                derived: true,
            },
        ],
    }
}

/// The painted result when no panel has a tree to draw.
pub(super) fn all_failed(ctx: &mut Ctx, failures: &[Option<String>], ratio: f64) -> Option<Painted> {
    if !failures.iter().all(Option::is_some) {
        return None;
    }
    let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
    let width = ctx.opts.width;
    let h = width / ratio;
    let panel = super::failed_panel(ctx, Rect::new(0.0, 0.0, width, h), &reason);
    Some(Painted {
        height: h,
        panels: vec![panel],
        figure_items: vec![],
        legend: None,
        failure: Some(reason),
    })
}
