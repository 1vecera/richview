//! Graph data for the charts that join named nodes by links (sankey, alluvial, chord, network):
//! rows become nodes and links, what cannot be drawn is left out with a finding, and the layered
//! flow layout that sankey and alluvial share paints columns of nodes joined by ribbons
//! (FEA-CH-75). Nothing is invented: a node's size is the sum of the links the author gave and a
//! readout marks it as derived (FEA-CH-42).

use super::hierarchy;
use super::pie::share_text;
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Stroke, Targeting,
};
use crate::numbers;
use crate::rows::{self, Num};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------------------------
// Reading rows into a graph.

/// How a chart treats the value of a link.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Values {
    /// The value is a flow: required, positive, and it sizes the ribbon.
    Flow,
    /// The value is an optional weight; a link without one weighs 1.
    Weight,
}

#[derive(Clone, Copy)]
pub(super) struct Rule {
    pub values: Values,
    /// Whether a link from a node to itself is kept.
    pub loops: bool,
}

pub(super) struct Link {
    pub source: usize,
    pub target: usize,
    pub value: f64,
    /// Whether the row gave the value.
    pub given: bool,
    pub row: usize,
}

/// What was left out of a graph, to be reported once for the chart.
#[derive(Default)]
pub(super) struct Left {
    /// Rows without a source, a target or a usable value.
    pub rows: usize,
    /// Links with a flow of zero, which have no width to draw.
    pub zero: usize,
    /// Links from a node to itself in a chart that cannot draw them.
    pub loops: usize,
    /// Rows of links that close a cycle.
    pub cycles: Vec<usize>,
}

impl Left {
    pub fn add(&mut self, other: Left) {
        self.rows += other.rows;
        self.zero += other.zero;
        self.loops += other.loops;
        self.cycles.extend(other.cycles);
    }
}

#[derive(Default)]
pub(super) struct Graph {
    /// Node names in the order they first appear, source before target.
    pub names: Vec<String>,
    pub links: Vec<Link>,
    pub failure: Option<String>,
    pub left: Left,
}

/// The rows of a panel the exact values drop before any link exists: values of zero and below.
/// Grid rows may snap a small value to zero, so the exact rows decide (TYP-36).
#[derive(Serialize, Deserialize)]
pub(super) struct Skipped {
    pub zero: Vec<usize>,
    pub negative: Vec<usize>,
}

/// The rows of `members` whose value in `field` is zero or negative, from the exact rows.
pub(super) fn skipped(ctx: &Ctx, panel: usize, members: &[usize], field: Option<&str>) -> Skipped {
    ctx.derive(panel, "graph.skipped", || {
        let mut skipped = Skipped {
            zero: Vec::new(),
            negative: Vec::new(),
        };
        if let Some(field) = field {
            for &i in members {
                match rows::num(&ctx.rows[i], field) {
                    Num::Value(v) if v < 0.0 => skipped.negative.push(i),
                    Num::Value(0.0) => skipped.zero.push(i),
                    _ => {}
                }
            }
        }
        skipped
    })
}

fn node(names: &mut Vec<String>, index: &mut HashMap<String, usize>, name: String) -> usize {
    if let Some(&k) = index.get(&name) {
        return k;
    }
    names.push(name.clone());
    index.insert(name, names.len() - 1);
    names.len() - 1
}

/// Read the rows of facet `panel` (`source`, `target` and, as the value, `y`). `what` names the
/// chart in failure reasons, such as `sankey`.
pub(super) fn read(ctx: &mut Ctx, panel: usize, members: &[usize], what: &str, rule: Rule) -> Graph {
    let block = ctx.block;
    let data = ctx.rows;
    let mut graph = Graph::default();
    let value_field = block.encoding.y.clone();
    let (Some(source_field), Some(target_field)) = (block.channel("source"), block.channel("target")) else {
        ctx.check(
            "source-error",
            Level::Error,
            format!("A {what} needs encoding.source and encoding.target"),
            Some("Name the columns that hold the start and the end of every link."),
            Some("chart.encoding.source".into()),
            None,
            Value::Null,
        );
        graph.failure = Some(format!(
            "Cannot draw this {what} · encoding.source and encoding.target are needed"
        ));
        return graph;
    };
    if rule.values == Values::Flow && value_field.is_none() {
        ctx.check(
            "source-error",
            Level::Error,
            format!("A {what} needs encoding.y, the column with the value of every link"),
            Some("Name the column that holds the values."),
            Some("chart.encoding.y".into()),
            None,
            Value::Null,
        );
        graph.failure = Some(format!("Cannot draw this {what} · encoding.y is needed"));
        return graph;
    }
    let skipped = skipped(ctx, panel, members, value_field.as_deref());
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut pairs: HashSet<(usize, usize)> = HashSet::new();
    for &i in members {
        let row = &data[i];
        let (Some(from), Some(to)) = (rows::text(row, &source_field), rows::text(row, &target_field)) else {
            graph.left.rows += 1;
            continue;
        };
        let value = match &value_field {
            None => None,
            Some(field) => match rows::num(row, field) {
                Num::Value(_) if skipped.negative.contains(&i) => {
                    graph.left.rows += 1;
                    continue;
                }
                Num::Value(_) if skipped.zero.contains(&i) => Some(0.0),
                // A value that is only zero on the grid still has a width.
                Num::Value(v) => Some(v.max(f64::MIN_POSITIVE)),
                Num::Missing => None,
                Num::Underflow(t) => {
                    ctx.underflow(i, field, &t);
                    graph.failure.get_or_insert_with(|| {
                        format!("Cannot draw this {what} · {field} has a value too small to represent")
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
                    graph.failure.get_or_insert_with(|| {
                        format!("Cannot draw this {what} · {field} has text that is not a number")
                    });
                    continue;
                }
            },
        };
        if rule.values == Values::Flow {
            match value {
                None => {
                    graph.left.rows += 1;
                    continue;
                }
                Some(0.0) => {
                    graph.left.zero += 1;
                    continue;
                }
                Some(_) => {}
            }
        }
        if from == to && !rule.loops {
            graph.left.loops += 1;
            continue;
        }
        let source = node(&mut graph.names, &mut index, from.clone());
        let target = node(&mut graph.names, &mut index, to.clone());
        if !pairs.insert((source, target)) {
            ctx.check(
                "source-error",
                Level::Error,
                format!(
                    "Row {} repeats the link {from} → {to}; a {what} needs one row for each pair",
                    i + 1
                ),
                Some("Give each pair of nodes one row."),
                Some(Ctx::row_path(i, &source_field)),
                None,
                json!({"row": i}),
            );
            graph
                .failure
                .get_or_insert_with(|| format!("Cannot draw this {what} · A link repeats"));
            continue;
        }
        graph.links.push(Link {
            source,
            target,
            value: value.unwrap_or(1.0),
            given: value.is_some(),
            row: i,
        });
    }
    finish_read(ctx, &mut graph, what, rule);
    graph
}

/// The checks every graph meets once its rows are read: a finite total, and something to draw.
pub(super) fn finish_read(ctx: &mut Ctx, graph: &mut Graph, what: &str, rule: Rule) {
    if graph.failure.is_some() {
        return;
    }
    let total: f64 = graph.links.iter().map(|l| l.value).sum();
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
        graph.failure = Some(format!("Cannot draw this {what} · The values are too large"));
    } else if graph.links.is_empty() {
        graph.failure = Some(if rule.values == Values::Flow {
            "Nothing to draw · No row has a source, a target and a value".to_owned()
        } else {
            "Nothing to draw · No row has a source and a target".to_owned()
        });
    }
}

/// Fail a graph that has more nodes or links than its chart type takes: it is not drawn, and
/// `check` raises `limit-exceeded` for each limit it passes (FEA-CH-75). Nothing is dropped.
pub(super) fn enforce_limits(ctx: &mut Ctx, graph: &mut Graph, chart: &str) {
    if graph.failure.is_some() {
        return;
    }
    let mut reason = None;
    for (what, count, path) in [
        ("nodes", graph.names.len(), "chart.encoding.source"),
        ("links", graph.links.len(), "chart.encoding.target"),
    ] {
        if count > crate::limits::max(chart, what) {
            let found = super::kit::over_limit(ctx, chart, what, count, Some(path));
            reason.get_or_insert(found);
        }
    }
    graph.failure = reason;
}

/// What a graph keeps once its cycles are broken, from the exact values: the rows of the links
/// that stay and the rows of the ones dropped (TYP-36).
#[derive(Serialize, Deserialize)]
struct Kept {
    rows: Vec<usize>,
    cycles: Vec<usize>,
}

impl Graph {
    /// When `acyclic`, break the graph's cycles. The choice jumps with the values, so the exact
    /// rows make it and grid rows take it from `ctx`.
    pub fn settle(&mut self, ctx: &Ctx, panel: usize, acyclic: bool) {
        if self.failure.is_some() || !acyclic {
            return;
        }
        let kept: Kept = ctx.derive(panel, "graph.kept", || {
            break_cycles(self);
            Kept {
                rows: self.links.iter().map(|l| l.row).collect(),
                cycles: self.left.cycles.clone(),
            }
        });
        let rows: HashSet<usize> = kept.rows.iter().copied().collect();
        self.links.retain(|l| rows.contains(&l.row));
        self.left.cycles = kept.cycles;
    }
}

/// Drop every link that closes a cycle, in row order, so what is left flows one way. The rows
/// that were dropped are kept in `left`.
pub(super) fn break_cycles(graph: &mut Graph) {
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); graph.names.len()];
    // `seen[k] == stamp` marks a node reached by the search of the current link.
    let mut seen = vec![0usize; graph.names.len()];
    let mut kept = Vec::with_capacity(graph.links.len());
    for (stamp, link) in std::mem::take(&mut graph.links).into_iter().enumerate() {
        if reaches(&next, &mut seen, stamp + 1, link.target, link.source) {
            graph.left.cycles.push(link.row);
        } else {
            next[link.source].push(link.target);
            kept.push(link);
        }
    }
    graph.links = kept;
}

/// Whether `to` can be reached from `from` along `next`; a node reaches itself.
fn reaches(next: &[Vec<usize>], seen: &mut [usize], stamp: usize, from: usize, to: usize) -> bool {
    if from == to {
        return true;
    }
    let mut stack = vec![from];
    seen[from] = stamp;
    while let Some(k) = stack.pop() {
        for &t in &next[k] {
            if t == to {
                return true;
            }
            if seen[t] != stamp {
                seen[t] = stamp;
                stack.push(t);
            }
        }
    }
    false
}

/// Say what was left out, once for the chart: rows, cycles and loops are findings; links of zero
/// flow are only noted, since they have nothing to show. `lacking` says what a left-out row
/// lacked and `path` is the channel to fix.
pub(super) fn report(ctx: &mut Ctx, left: &Left, what: &str, lacking: &str, path: &str) {
    if left.rows > 0 {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} without {lacking} {} left out",
                super::count(left.rows, "row", "rows"),
                super::verb(left.rows, "is", "are")
            ),
            Some("Fill in every field the chart reads."),
            Some(path.to_owned()),
            None,
            json!({"count": left.rows}),
        );
        ctx.note.push(format!(
            "{} without a usable value {} not shown",
            super::count(left.rows, "row", "rows"),
            super::verb(left.rows, "is", "are")
        ));
    }
    if !left.cycles.is_empty() {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} a cycle and {} left out; a {what} flows one way",
                super::count(left.cycles.len(), "link closes", "links close"),
                super::verb(left.cycles.len(), "is", "are")
            ),
            Some("Remove the link that points back."),
            Some("chart.encoding.target".into()),
            None,
            json!({"count": left.cycles.len(), "rows": left.cycles}),
        );
        ctx.note.push(format!(
            "{} a cycle {} not shown",
            super::count(left.cycles.len(), "link that closes", "links that close"),
            super::verb(left.cycles.len(), "is", "are")
        ));
    }
    if left.loops > 0 {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} from a node to itself {} left out",
                super::count(left.loops, "link", "links"),
                super::verb(left.loops, "is", "are")
            ),
            Some("Give each link two different nodes."),
            Some("chart.encoding.target".into()),
            None,
            json!({"count": left.loops}),
        );
        ctx.note.push(format!(
            "{} from a node to itself {} not shown",
            super::count(left.loops, "link", "links"),
            super::verb(left.loops, "is", "are")
        ));
    }
    if left.zero > 0 {
        ctx.note.push(format!(
            "{} with a value of zero {} not drawn",
            super::count(left.zero, "link", "links"),
            super::verb(left.zero, "is", "are")
        ));
    }
}

// ---------------------------------------------------------------------------------------------
// The layered flow layout of sankey and alluvial.

/// Width of a node bar.
const NODE_W: f64 = 10.0;
/// Thinnest a ribbon is drawn, so a small flow stays visible and can be pointed at.
const MIN_RIBBON: f64 = 0.75;
/// Opacity of a ribbon, so crossings stay readable.
const RIBBON_ALPHA: f64 = 0.4;
/// Passes of the ordering and relaxation sweeps.
const SWEEPS: usize = 8;

pub(super) struct FlowNode {
    pub name: String,
    /// Column from the left, 0 first.
    pub column: usize,
    /// The colour slot of the node, `None` for a neutral bar.
    pub slot: Option<usize>,
}

pub(super) struct FlowLink {
    pub source: usize,
    pub target: usize,
    pub value: f64,
    /// The colour slot of the ribbon.
    pub slot: usize,
    /// The row behind the link, `None` when rows were added up.
    pub row: Option<usize>,
    /// A group the ribbon belongs to, shown in its readout.
    pub group: Option<String>,
}

/// One panel's nodes and links; every link joins an earlier column to a later one.
pub(super) struct Flow {
    pub nodes: Vec<FlowNode>,
    pub links: Vec<FlowLink>,
    pub columns: usize,
    /// A title over each column, or none.
    pub titles: Vec<String>,
}

impl Flow {
    /// Most nodes in any column.
    fn most(&self) -> usize {
        let mut counts = vec![0usize; self.columns.max(1)];
        for n in &self.nodes {
            let last = counts.len() - 1;
            counts[n.column.min(last)] += 1;
        }
        counts.into_iter().max().unwrap_or(1)
    }
}

/// What differs between the charts that paint a flow.
pub(super) struct FlowLook {
    /// Width over height when the author gives none.
    pub ratio: f64,
    /// Whether to reorder columns and relax nodes towards what they join (sankey).
    pub relax: bool,
    /// The name of a value in a readout.
    pub value_name: String,
    /// Whether a ribbon's value was added up by RichView.
    pub derived_links: bool,
    /// The name of the share a node reads out.
    pub share_name: &'static str,
    /// The name of the group line in a ribbon's readout, when ribbons have groups.
    pub group_name: Option<String>,
}

/// What a legend toggle leaves visible of one flow.
struct View {
    link_on: Vec<bool>,
    node_on: Vec<bool>,
    input: Vec<f64>,
    output: Vec<f64>,
    /// The links that enter and leave each node, by link index.
    into: Vec<Vec<usize>>,
    out_of: Vec<Vec<usize>>,
    /// The least a node is drawn tall: a pixel, and room for the thinnest ribbon of each link.
    floor: Vec<f64>,
}

impl View {
    fn new(ctx: &Ctx, flow: &Flow) -> View {
        let link_on: Vec<bool> = flow.links.iter().map(|l| !ctx.is_hidden(l.slot)).collect();
        Self::from_links(flow, link_on)
    }

    fn from_links(flow: &Flow, link_on: Vec<bool>) -> View {
        let mut node_on = vec![false; flow.nodes.len()];
        let mut input = vec![0.0; flow.nodes.len()];
        let mut output = vec![0.0; flow.nodes.len()];
        let mut into: Vec<Vec<usize>> = vec![Vec::new(); flow.nodes.len()];
        let mut out_of: Vec<Vec<usize>> = vec![Vec::new(); flow.nodes.len()];
        for (li, (l, on)) in flow.links.iter().zip(&link_on).enumerate() {
            if *on {
                node_on[l.source] = true;
                node_on[l.target] = true;
                output[l.source] += l.value;
                input[l.target] += l.value;
                out_of[l.source].push(li);
                into[l.target].push(li);
            }
        }
        let floor = (0..flow.nodes.len())
            .map(|k| (into[k].len().max(out_of[k].len()) as f64 * MIN_RIBBON).max(1.0))
            .collect();
        View {
            link_on,
            node_on,
            input,
            output,
            into,
            out_of,
            floor,
        }
    }

    /// The size of a node: the larger of what enters and what leaves it.
    fn through(&self, k: usize) -> f64 {
        self.input[k].max(self.output[k])
    }

    fn any(&self) -> bool {
        self.link_on.iter().any(|on| *on)
    }
}

/// The summed size and the node count of every column.
fn column_totals(flow: &Flow, view: &View) -> (Vec<f64>, Vec<usize>) {
    let mut totals = vec![0.0; flow.columns];
    let mut counts = vec![0usize; flow.columns];
    for (k, n) in flow.nodes.iter().enumerate() {
        if view.node_on[k] {
            totals[n.column] += view.through(k);
            counts[n.column] += 1;
        }
    }
    (totals, counts)
}

/// The tallest a column of values `norm` can be scaled to: pixels per `norm`. Every node is at
/// least its floor tall, so the floors are part of what has to fit.
fn scale_of(flow: &Flow, view: &View, norm: f64, avail: f64, pad: f64) -> f64 {
    let (totals, counts) = column_totals(flow, view);
    let mut scale = f64::INFINITY;
    for (c, (total, count)) in totals.iter().zip(&counts).enumerate() {
        if *total > 0.0 && *count > 0 {
            let room = (avail - pad * (*count as f64 - 1.0)).max(1.0);
            let nodes: Vec<(f64, f64)> = (0..flow.nodes.len())
                .filter(|&k| view.node_on[k] && flow.nodes[k].column == c)
                .map(|k| (view.through(k), view.floor[k]))
                .collect();
            let whole = room * (norm / total);
            let fits = |s: f64| nodes.iter().map(|&(t, f)| (t / norm * s).max(f)).sum::<f64>() <= room;
            let column = if nodes.iter().all(|&(t, f)| t / norm * whole >= f) {
                whole
            } else if !fits(0.0) {
                0.0
            } else {
                let (mut lo, mut hi) = (0.0, whole);
                for _ in 0..60 {
                    let mid = (lo + hi) / 2.0;
                    if fits(mid) {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                lo
            };
            scale = scale.min(column);
        }
    }
    if scale.is_finite() { scale } else { 0.0 }
}

/// Where the nodes of one panel may stand.
struct Bay {
    top: f64,
    avail: f64,
    pad: f64,
}

/// Nodes in each column, reordered by the average position of what they join, sweeping back and
/// forth: fewer ribbons cross.
fn sweep(flow: &Flow, view: &View, unit: f64, order: &mut [Vec<usize>]) {
    let mut rank = vec![0.0; flow.nodes.len()];
    ranks(order, &mut rank);
    for pass in 0..SWEEPS {
        let forward = pass % 2 == 0;
        let columns: Vec<usize> = if forward {
            (1..order.len()).collect()
        } else {
            (0..order.len().saturating_sub(1)).rev().collect()
        };
        for c in columns {
            let mut keyed: Vec<(f64, usize)> = order[c]
                .iter()
                .map(|&k| (pull(flow, view, unit, &rank, k, forward).unwrap_or(rank[k]), k))
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            order[c] = keyed.into_iter().map(|p| p.1).collect();
            ranks(order, &mut rank);
        }
    }
}

fn ranks(order: &[Vec<usize>], rank: &mut [f64]) {
    for column in order {
        let len = column.len() as f64;
        for (i, &k) in column.iter().enumerate() {
            rank[k] = (i as f64 + 0.5) / len;
        }
    }
}

/// The average of `at` over the nodes `k` joins on one side, weighted by the links' values.
fn pull(flow: &Flow, view: &View, unit: f64, at: &[f64], k: usize, from_before: bool) -> Option<f64> {
    let (mut sum, mut weight) = (0.0, 0.0);
    let joined = if from_before { &view.into[k] } else { &view.out_of[k] };
    for &li in joined {
        let l = &flow.links[li];
        let other = if from_before { l.source } else { l.target };
        let w = l.value / unit;
        sum += at[other] * w;
        weight += w;
    }
    (weight > 0.0).then(|| sum / weight)
}

/// Push the nodes of a column apart so none overlaps another or leaves the bay.
fn settle(column: &mut [usize], top: &mut [f64], height: &[f64], bay: &Bay) {
    column.sort_by(|&a, &b| top[a].total_cmp(&top[b]).then(a.cmp(&b)));
    let mut y = bay.top;
    for &k in column.iter() {
        if top[k] < y {
            top[k] = y;
        }
        y = top[k] + height[k] + bay.pad;
    }
    let mut y = bay.top + bay.avail;
    for &k in column.iter().rev() {
        if top[k] + height[k] > y {
            top[k] = y - height[k];
        }
        y = top[k] - bay.pad;
    }
}

/// Move every node towards the weighted centre of the nodes it joins, sweeping back and forth
/// with a fading pull.
fn relax(flow: &Flow, view: &View, unit: f64, order: &mut [Vec<usize>], top: &mut [f64], height: &[f64], bay: &Bay) {
    for pass in 0..SWEEPS {
        let forward = pass % 2 == 0;
        let alpha = 0.9 - 0.1 * pass as f64;
        let columns: Vec<usize> = if forward {
            (1..order.len()).collect()
        } else {
            (0..order.len().saturating_sub(1)).rev().collect()
        };
        for c in columns {
            let centres: Vec<f64> = (0..flow.nodes.len()).map(|k| top[k] + height[k] / 2.0).collect();
            for &k in &order[c] {
                if let Some(want) = pull(flow, view, unit, &centres, k, forward) {
                    top[k] += (want - height[k] / 2.0 - top[k]) * alpha;
                }
            }
            let mut column = order[c].clone();
            settle(&mut column, top, height, bay);
            order[c] = column;
        }
    }
}

/// The thickest link: the unit the sweeps weigh links in.
fn unit_of(flow: &Flow) -> f64 {
    flow.links
        .iter()
        .map(|l| l.value)
        .fold(0.0, f64::max)
        .max(f64::MIN_POSITIVE)
}

/// The nodes of every column from top to bottom, after the sweeps when `relaxed`. This order
/// jumps with the values, so the exact rows decide it (TYP-36).
fn column_order(flow: &Flow, view: &View, relaxed: bool) -> Vec<Vec<usize>> {
    let mut order: Vec<Vec<usize>> = vec![Vec::new(); flow.columns];
    for (k, node) in flow.nodes.iter().enumerate() {
        if view.node_on[k] {
            order[node.column].push(k);
        }
    }
    if relaxed {
        sweep(flow, view, unit_of(flow), &mut order);
    }
    order
}

/// The readout texts of sums and shares RichView computed, from the exact values.
#[derive(Serialize, Deserialize)]
struct Texts {
    /// Per node: the size, what enters, what leaves and the share of its column; empty when the
    /// readout has no such line.
    nodes: Vec<[String; 4]>,
    /// Per link: its value (a sum when rows were added up), the share of its source and of its
    /// target; the shares are empty when there is none.
    links: Vec<[String; 3]>,
}

fn texts_of(ctx: &Ctx, flow: &Flow, view: &View, readout: &numbers::Format) -> Texts {
    let (totals, _) = column_totals(flow, view);
    let nodes = (0..flow.nodes.len())
        .map(|k| {
            let through = view.through(k);
            let both = view.input[k] > 0.0 && view.output[k] > 0.0;
            let column = totals[flow.nodes[k].column];
            [
                numbers::value(through, readout),
                if both {
                    numbers::value(view.input[k], readout)
                } else {
                    String::new()
                },
                if both {
                    numbers::value(view.output[k], readout)
                } else {
                    String::new()
                },
                if column > 0.0 {
                    share_text(through / column, ctx.locale)
                } else {
                    String::new()
                },
            ]
        })
        .collect();
    let links = flow
        .links
        .iter()
        .map(|l| {
            [
                numbers::value(l.value, readout),
                if view.output[l.source] > 0.0 {
                    share_text(l.value / view.output[l.source], ctx.locale)
                } else {
                    String::new()
                },
                if view.input[l.target] > 0.0 {
                    share_text(l.value / view.input[l.target], ctx.locale)
                } else {
                    String::new()
                },
            ]
        })
        .collect();
    Texts { nodes, links }
}

/// Tops and heights of every node, the columns ordered top to bottom as `order` says.
fn arrange(
    flow: &Flow,
    view: &View,
    norm: f64,
    scale: f64,
    bay: &Bay,
    relaxed: bool,
    mut order: Vec<Vec<usize>>,
) -> (Vec<f64>, Vec<f64>) {
    let n = flow.nodes.len();
    let height: Vec<f64> = (0..n)
        .map(|k| {
            if view.node_on[k] {
                (view.through(k) / norm * scale).max(view.floor[k])
            } else {
                0.0
            }
        })
        .collect();
    let unit = unit_of(flow);
    let mut top = vec![0.0; n];
    for column in &order {
        let used: f64 =
            column.iter().map(|&k| height[k]).sum::<f64>() + bay.pad * column.len().saturating_sub(1) as f64;
        let mut y = bay.top + ((bay.avail - used) / 2.0).max(0.0);
        for &k in column {
            top[k] = y;
            y += height[k] + bay.pad;
        }
    }
    if relaxed {
        relax(flow, view, unit, &mut order, &mut top, &height, bay);
    }
    (top, height)
}

/// Where each link's ribbon leaves its source and enters its target, and how thick it is.
fn slots(flow: &Flow, view: &View, top: &[f64], height: &[f64], thickness: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = flow.nodes.len();
    let centre = |k: usize| top[k] + height[k] / 2.0;
    let mut out_of: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut into: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (li, l) in flow.links.iter().enumerate() {
        if view.link_on[li] {
            out_of[l.source].push(li);
            into[l.target].push(li);
        }
    }
    let mut from = vec![0.0; flow.links.len()];
    let mut to = vec![0.0; flow.links.len()];
    for k in 0..n {
        out_of[k].sort_by(|&a, &b| {
            centre(flow.links[a].target)
                .total_cmp(&centre(flow.links[b].target))
                .then(a.cmp(&b))
        });
        let mut y = top[k];
        for &li in &out_of[k] {
            from[li] = y;
            y += thickness[li];
        }
        into[k].sort_by(|&a, &b| {
            centre(flow.links[a].source)
                .total_cmp(&centre(flow.links[b].source))
                .then(a.cmp(&b))
        });
        let mut y = top[k];
        for &li in &into[k] {
            to[li] = y;
            y += thickness[li];
        }
    }
    (from, to)
}

/// Paint the panels of a flow chart: columns of nodes, ribbons as wide as their flow, names
/// beside the nodes, and an observation for every node and every ribbon.
pub(super) fn paint_flows(ctx: &mut Ctx, facets: &Facets, flows: &[Result<Flow, String>], look: &FlowLook) -> Painted {
    let block = ctx.block;
    let failures: Vec<Option<String>> = flows.iter().map(|f| f.as_ref().err().cloned()).collect();
    if let Some(painted) = hierarchy::all_failed(ctx, &failures, look.ratio) {
        return painted;
    }
    let readout = block.readout_format(&block.y, ctx.locale);
    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let header = if flows.iter().flatten().any(|f| !f.titles.is_empty()) {
        lh + 8.0
    } else {
        0.0
    };
    let tallest = flows.iter().flatten().map(Flow::most).max().unwrap_or(1);
    let natural = pw / look.ratio;
    let plot_h = if block.style.ratio.is_some() {
        natural
    } else {
        natural.max((tallest as f64 * (lh + 8.0) + header + 16.0).min(pw * 1.2))
    };
    let panel_h = plot_h + title_room;
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let avail = (plot_h - header - 6.0).max(1.0);
    let pad = if tallest > 1 {
        (avail * 0.3 / (tallest as f64 - 1.0)).min(14.0)
    } else {
        0.0
    };
    // One scale for every panel, unless the author frees them: the same value is as tall in each.
    let mut views: Vec<Option<View>> = Vec::new();
    for flow in flows {
        views.push(flow.as_ref().ok().map(|f| View::new(ctx, f)));
    }
    let norm_of = |flow: &Flow, view: &View| -> f64 {
        let (totals, _) = column_totals(flow, view);
        totals.into_iter().fold(0.0, f64::max)
    };
    let mut global = 0.0f64;
    for (flow, view) in flows.iter().zip(&views) {
        if let (Ok(flow), Some(view)) = (flow, view) {
            global = global.max(norm_of(flow, view));
        }
    }
    let free = block.style.free_scales;
    let mut shared = f64::INFINITY;
    for (flow, view) in flows.iter().zip(&views) {
        if let (Ok(flow), Some(view)) = (flow, view) {
            let norm = if free { norm_of(flow, view) } else { global };
            if norm > 0.0 {
                shared = shared.min(scale_of(flow, view, norm, avail, pad));
            }
        }
    }
    let surface = Style::rgba(ctx.style.surface);
    let mut panels = Vec::new();
    for (fi, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        let (flow, view) = match (&flows[fi], &views[fi]) {
            (Ok(flow), Some(view)) => (flow, view),
            (Err(reason), _) => {
                panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
                continue;
            }
            _ => continue,
        };
        if !view.any() {
            panels.push(super::failed_facet(
                ctx,
                *frame,
                title_room,
                name,
                "Nothing to draw · Every group is hidden",
            ));
            continue;
        }
        let plot = Rect::new(frame.x, frame.y + title_room, frame.w, frame.h - title_room);
        let own = norm_of(flow, view);
        let (norm, scale) = if free {
            (own, scale_of(flow, view, own, avail, pad))
        } else {
            (global, shared)
        };
        let last = flow.columns.saturating_sub(1);
        let wide = |column: usize| -> f64 {
            flow.nodes
                .iter()
                .enumerate()
                .filter(|(k, n)| view.node_on[*k] && n.column == column)
                .map(|(_, n)| ctx.text_width(&n.name))
                .fold(0.0, f64::max)
        };
        let left_room = (wide(0) + 9.0).min(plot.w * 0.3);
        let right_room = (wide(last) + 9.0).min(plot.w * 0.3);
        let left = plot.x + left_room + 2.0;
        let right = plot.right() - right_room - 2.0;
        // Bars narrow when many columns share the width, so a ribbon always has room between them.
        let run = (right - left).max(0.0);
        let nw = (run / (2.0 * flow.columns as f64 - 1.0)).clamp(1.0, NODE_W);
        let spacing = if flow.columns > 1 {
            (run - nw).max(0.0) / (flow.columns as f64 - 1.0)
        } else {
            0.0
        };
        let xs: Vec<f64> = (0..flow.columns).map(|c| left + c as f64 * spacing).collect();
        let bay = Bay {
            top: plot.y + header + 3.0,
            avail,
            pad,
        };
        let order = ctx.derive(fi, "flow.order", || {
            column_order(flow, &View::from_links(flow, vec![true; flow.links.len()]), look.relax)
        });
        let texts = ctx.derive_visible(fi, "flow.texts", || texts_of(ctx, flow, view, &readout));
        let (top, height) = arrange(flow, view, norm, scale, &bay, look.relax, order);
        let thickness: Vec<f64> = flow
            .links
            .iter()
            .map(|l| (l.value / norm * scale).max(MIN_RIBBON))
            .collect();
        let (from, to) = slots(flow, view, &top, &height, &thickness);
        let nodes_on: Vec<usize> = (0..flow.nodes.len()).filter(|&k| view.node_on[k]).collect();
        // Observations: nodes first, then ribbons.
        let mut node_observations: Vec<Observation> = Vec::new();
        let mut link_observations: Vec<Observation> = Vec::new();
        let mut ribbons: Vec<Item> = Vec::new();
        let mut bars: Vec<Item> = Vec::new();
        let first_link = nodes_on.len();
        for (li, l) in flow.links.iter().enumerate() {
            if !view.link_on[li] {
                continue;
            }
            let (s, t) = (&flow.nodes[l.source], &flow.nodes[l.target]);
            let x0 = xs[s.column] + nw;
            let x1 = xs[t.column];
            if x1 < x0 {
                continue;
            }
            let mid = (x0 + x1) / 2.0;
            let (ya, yb, th) = (from[li], to[li], thickness[li]);
            let mut path = Path::new();
            path.move_to([x0, ya]);
            path.curve_to([mid, ya], [mid, yb], [x1, yb]);
            path.line_to([x1, yb + th]);
            path.curve_to([mid, yb + th], [mid, ya + th], [x0, ya + th]);
            path.close();
            let at = [mid, (ya + yb) / 2.0 + th / 2.0];
            let area = path.bounds().unwrap_or(Rect::new(x0, ya, 0.0, th));
            let hatch = ctx.series.get(l.slot).and_then(|s| s.hatch);
            let obs = first_link + link_observations.len();
            ribbons.push(Item::Area {
                path,
                fill: Fill {
                    color: Style::rgba(ctx.series_color(l.slot)).with_alpha(RIBBON_ALPHA),
                    hatch,
                    hatch_ink: hatch.map(|_| surface),
                    hatch_scale: 1.0,
                },
                role: Role::Mark,
                series: Some(l.slot),
                obs: Some(obs),
            });
            let mut lines = vec![ReadoutLine {
                label: None,
                name: look.value_name.clone(),
                value: texts.links[li][0].clone(),
                series: Some(l.slot),
                names_series: false,
                derived: look.derived_links,
            }];
            if let (Some(group_name), Some(group)) = (&look.group_name, &l.group) {
                lines.push(ReadoutLine {
                    label: None,
                    name: group_name.clone(),
                    value: group.clone(),
                    series: None,
                    names_series: false,
                    derived: false,
                });
            }
            for (name, text) in ["Share of source", "Share of target"]
                .into_iter()
                .zip(&texts.links[li][1..])
            {
                if !text.is_empty() {
                    lines.push(ReadoutLine {
                        label: None,
                        name: name.into(),
                        value: text.clone(),
                        series: None,
                        names_series: false,
                        derived: true,
                    });
                }
            }
            link_observations.push(Observation {
                row: l.row,
                dots: vec![],
                series: Some(l.slot),
                at,
                wedge: None,
                area: Some(area),
                radius: 0.0,
                x: at[0],
                readout: Readout {
                    heading: format!("{} → {}", s.name, t.name),
                    lines,
                }
                .into(),
            });
        }
        for &k in &nodes_on {
            let n = &flow.nodes[k];
            let rect = Rect::new(xs[n.column], top[k], nw, height[k]);
            let color = match n.slot {
                Some(slot) => ctx.series_color(slot),
                None => ctx.style.ink_soft,
            };
            let hatch = n.slot.and_then(|slot| ctx.series.get(slot).and_then(|s| s.hatch));
            bars.push(Item::Rect {
                rect,
                fill: Fill {
                    color: Style::rgba(color),
                    hatch,
                    hatch_ink: hatch.map(|_| surface),
                    hatch_scale: 1.0,
                },
                stroke: Some(Stroke::solid(surface, 1.0)),
                radius: [0.0; 4],
                role: Role::Mark,
                series: n.slot,
                obs: Some(node_observations.len()),
            });
            let [through, input, output, share] = &texts.nodes[k];
            let mut lines = vec![ReadoutLine {
                label: None,
                name: look.value_name.clone(),
                value: through.clone(),
                series: n.slot,
                names_series: false,
                derived: true,
            }];
            for (name, text) in [("In", input), ("Out", output), (look.share_name, share)] {
                if !text.is_empty() {
                    lines.push(ReadoutLine {
                        label: None,
                        name: name.into(),
                        value: text.clone(),
                        series: None,
                        names_series: false,
                        derived: true,
                    });
                }
            }
            let at = rect.center();
            node_observations.push(Observation {
                row: None,
                dots: vec![],
                series: n.slot,
                at,
                wedge: None,
                area: Some(rect),
                radius: 0.0,
                x: at[0],
                readout: Readout {
                    heading: match flow.titles.get(n.column) {
                        Some(title) => format!("{title}: {}", n.name),
                        None => n.name.clone(),
                    },
                    lines,
                }
                .into(),
            });
        }
        // Column titles, then the names of the nodes, largest first so that the small ones give way.
        let mut texts: Vec<Item> = Vec::new();
        let mut taken: Vec<Rect> = Vec::new();
        for (c, title) in flow.titles.iter().enumerate() {
            let centre = xs[c] + nw / 2.0;
            let y = plot.y + 3.0 + lh / 2.0;
            let tries = [
                (Anchor::Middle, centre),
                (Anchor::Start, xs[c]),
                (Anchor::End, xs[c] + nw),
            ];
            let mut placed = false;
            for (anchor, x) in tries {
                let t = ctx.text_mid(title, x, y, anchor, Face::Data, Role::AxisLabel, ctx.style.ink_soft);
                if t.bounds.inside(&plot, 0.0) && taken.iter().all(|o| !o.overlaps(&t.bounds, 2.0)) {
                    taken.push(t.bounds);
                    texts.push(Item::Text(t));
                    placed = true;
                    break;
                }
            }
            if !placed {
                let b = ctx.text_mid(
                    title,
                    centre,
                    y,
                    Anchor::Middle,
                    Face::Data,
                    Role::AxisLabel,
                    ctx.style.ink_soft,
                );
                ctx.drop_label(title, DropReason::Collision, "column", Some(b.bounds));
            }
        }
        let mut by_size = nodes_on.clone();
        by_size.sort_by(|&a, &b| height[b].total_cmp(&height[a]).then(a.cmp(&b)));
        for k in by_size {
            let n = &flow.nodes[k];
            let cy = top[k] + height[k] / 2.0;
            let (anchor, x) = if n.column == 0 {
                (Anchor::End, xs[0] - 5.0)
            } else {
                (Anchor::Start, xs[n.column] + nw + 5.0)
            };
            let t = ctx.text_mid(&n.name, x, cy, anchor, Face::Data, Role::Label, ctx.style.ink);
            let b = t.bounds;
            // A name between columns must end before the next column's bars.
            let before_next = n.column == last || n.column + 1 >= xs.len() || b.right() <= xs[n.column + 1] - 3.0;
            if before_next && b.inside(&plot, 0.0) && taken.iter().all(|o| !o.overlaps(&b, 0.0)) {
                taken.push(b);
                texts.push(Item::Text(t));
            } else {
                ctx.drop_label(&n.name, DropReason::Collision, "node", Some(b));
            }
        }
        let mut items = ribbons;
        items.extend(bars);
        items.extend(texts);
        let mut observations = node_observations;
        observations.extend(link_observations);
        let mut panel = Panel {
            facet: None,
            frame: *frame,
            plot,
            items,
            observations,
            targeting: Targeting::Contain,
            x: None,
            failure: None,
        };
        super::panel_title(ctx, &mut panel, name);
        panels.push(panel);
    }
    Painted {
        height: super::grid_height(facets.len(), columns, panel_h, super::FACET_GAP_Y),
        panels,
        figure_items: vec![],
        legend: None,
        failure: None,
    }
}
