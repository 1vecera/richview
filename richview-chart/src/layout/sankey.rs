//! Sankey diagrams: flows between named nodes as ribbons as wide as their values (FEA-CH-75).
//! A node stands in the column of its longest path from a source, is as tall as the larger of
//! what enters and what leaves it, and takes the colour of the source its heaviest inflow
//! comes from. A link that closes a cycle is left out with a finding, so what is drawn flows one
//! way. Node sizes are sums RichView computed and readouts mark them as derived (FEA-CH-42).

use super::graph::{self, Flow, FlowLink, FlowLook, FlowNode, Graph, Left, Rule, Values};
use super::kit;
use super::types::Facets;
use super::{Ctx, Painted};
use crate::geometry::Swatch;

const RATIO: f64 = 16.0 / 9.0;

/// A graph's columns: the longest path from a source, and the nodes in topological order.
struct Layers {
    column: Vec<usize>,
    /// Nodes with links out and none in.
    roots: Vec<usize>,
    order: Vec<usize>,
}

fn layers(g: &Graph) -> Layers {
    let n = g.names.len();
    let mut indegree = vec![0usize; n];
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
    for l in &g.links {
        indegree[l.target] += 1;
        next[l.source].push(l.target);
    }
    let roots: Vec<usize> = (0..n).filter(|&k| indegree[k] == 0 && !next[k].is_empty()).collect();
    let mut column = vec![0usize; n];
    let mut order = roots.clone();
    let mut head = 0;
    while head < order.len() {
        let k = order[head];
        head += 1;
        for &t in &next[k] {
            column[t] = column[t].max(column[k] + 1);
            indegree[t] -= 1;
            if indegree[t] == 0 {
                order.push(t);
            }
        }
    }
    Layers { column, roots, order }
}

/// One panel's flow: colours follow the root a node's heaviest inflow comes from. Which inflow
/// is heaviest jumps with the values, so the exact rows decide the slots (TYP-36).
fn build(ctx: &Ctx, panel: usize, g: &Graph, roots: &[String]) -> Flow {
    let n = g.names.len();
    let layers = layers(g);
    let slot: Vec<usize> = ctx.derive(panel, "sankey.slots", || {
        let mut heaviest: Vec<Option<usize>> = vec![None; n];
        for (li, l) in g.links.iter().enumerate() {
            let current = heaviest[l.target];
            if current.is_none_or(|b| g.links[b].value < l.value) {
                heaviest[l.target] = Some(li);
            }
        }
        let mut slot = vec![0usize; n];
        for &k in &layers.order {
            slot[k] = match heaviest[k] {
                Some(li) => slot[g.links[li].source],
                None => roots.iter().position(|r| *r == g.names[k]).unwrap_or(0),
            };
        }
        slot
    });
    Flow {
        nodes: (0..n)
            .map(|k| FlowNode {
                name: g.names[k].clone(),
                column: layers.column[k],
                slot: Some(slot[k]),
            })
            .collect(),
        links: g
            .links
            .iter()
            .map(|l| FlowLink {
                source: l.source,
                target: l.target,
                value: l.value,
                slot: slot[l.source],
                row: Some(l.row),
                group: None,
            })
            .collect(),
        columns: layers.column.iter().copied().max().unwrap_or(0) + 1,
        titles: Vec::new(),
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let rule = Rule {
        values: Values::Flow,
        loops: false,
    };
    let mut graphs: Vec<Graph> = Vec::new();
    let mut left = Left::default();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut g = graph::read(ctx, fi, members, "sankey", rule);
        graph::enforce_limits(ctx, &mut g, "sankey");
        g.settle(ctx, fi, true);
        left.add(std::mem::take(&mut g.left));
        graphs.push(g);
    }
    graph::report(
        ctx,
        &left,
        "sankey",
        "a source, a target or a non-negative value",
        "chart.encoding.source",
    );
    // Colour slots: the sources, by name across panels in first-seen order.
    let mut roots: Vec<String> = Vec::new();
    for g in graphs.iter().filter(|g| g.failure.is_none()) {
        for &r in &layers(g).roots {
            if !roots.contains(&g.names[r]) {
                roots.push(g.names[r].clone());
            }
        }
    }
    ctx.set_series(&roots);
    let flows: Vec<Result<Flow, String>> = graphs
        .iter()
        .enumerate()
        .map(|(fi, g)| match &g.failure {
            Some(reason) => Err(reason.clone()),
            None => Ok(build(ctx, fi, g, &roots)),
        })
        .collect();
    let (mut links, mut nodes) = (0usize, 0usize);
    for f in flows.iter().flatten() {
        links += f.links.len();
        let mut touched = vec![false; f.nodes.len()];
        for l in &f.links {
            touched[l.source] = true;
            touched[l.target] = true;
        }
        nodes += touched.iter().filter(|t| **t).count();
    }
    ctx.summary = format!(
        "Sankey diagram of {} between {}.",
        super::count(links, "flow", "flows"),
        super::count(nodes, "node", "nodes")
    );
    ctx.note.push(
        "ribbons are as wide as their flow; a node is as tall as the larger of what enters and what leaves it".into(),
    );
    let value_name = block
        .y
        .label
        .clone()
        .or_else(|| block.encoding.y.as_deref().map(super::cartesian::readable_name))
        .unwrap_or_else(|| "Value".into());
    let look = FlowLook {
        ratio: block.style.ratio.unwrap_or(RATIO),
        relax: true,
        value_name,
        derived_links: false,
        share_name: "Share of column",
        group_name: None,
    };
    let mut painted = graph::paint_flows(ctx, facets, &flows, &look);
    if painted.failure.is_none() {
        painted.legend = kit::legend(ctx, Swatch::Square);
    }
    painted
}
