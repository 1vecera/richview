//! Flow charts (FEA-CH-76): cards joined by right-angled lines, laid out in ranks along the flow,
//! a native port of NovaFlow's layered layout. An author writes `nodes`, `edges` and `groups` in
//! the block; a flow drawn from data takes rows through `encoding` instead (steps by `label`,
//! connections by `source` and `target`, lanes by `lane`, connection labels by `y`).
//!
//! Every card is measured with the style's real faces before anything is placed; [`engine`] then
//! places cards, groups and lines in abstract axes, and this module draws its result in page
//! axes: down the page by default, across it with `direction: right`. Nothing is random: the
//! same block gives the same geometry.

mod engine;

use super::kit;
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::color::{self, Oklch};
use crate::diag::Level;
use crate::geometry::{
    Anchor, Cmd, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Stroke,
    Targeting, Text,
};
use crate::limits;
use crate::rows::{self, Num};
use crate::text::Font;
use engine::{EdgeIn, GroupIn, Input, NodeIn, Params, Ports};
use serde_json::{Value, json};

/// What a card stands for; each has one outline so a reader tells them apart without a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Something done: a rounded card.
    Step,
    /// A question with labelled exits: a diamond.
    Decision,
    /// Where the flow begins: a pill.
    Start,
    /// Where it ends: a pill.
    End,
    /// Data at rest the flow reads or writes: a cylinder.
    Store,
    /// A thing a step makes or hands on, such as a file or a model: a sheet with a folded corner.
    Object,
}

impl Kind {
    fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "step" => Kind::Step,
            "decision" => Kind::Decision,
            "start" => Kind::Start,
            "end" => Kind::End,
            "store" => Kind::Store,
            "object" => Kind::Object,
            _ => return None,
        })
    }

    fn noun(self) -> &'static str {
        match self {
            Kind::Step => "Step",
            Kind::Decision => "Decision",
            Kind::Start => "Start",
            Kind::End => "End",
            Kind::Store => "Store",
            Kind::Object => "Object",
        }
    }
}

/// A colour that carries one meaning across the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    /// The group's colour, or neutral outside a group.
    Plain,
    /// What needs the reader's eye: a failure, a risk.
    Alert,
    /// Outside the subject: a source, a system nobody here runs.
    Muted,
}

impl Tone {
    fn parse(s: &str) -> Option<Tone> {
        match s {
            "alert" => Some(Tone::Alert),
            "muted" => Some(Tone::Muted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct Node {
    id: String,
    title: String,
    lines: Vec<String>,
    kind: Kind,
    tone: Tone,
    group: Option<usize>,
    lane: Option<usize>,
    row: Option<usize>,
}

#[derive(Debug, Clone)]
struct Edge {
    from: usize,
    to: usize,
    label: Option<String>,
    dashed: bool,
    tone: Tone,
    row: Option<usize>,
}

#[derive(Debug, Clone)]
struct Group {
    label: String,
    tone: Tone,
}

#[derive(Debug, Clone, Default)]
struct Flow {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    groups: Vec<Group>,
    lanes: Vec<String>,
    across: bool,
    /// Written as `nodes` and `edges`: a loop is drawn as written. From data rows, a connection
    /// that closes a cycle is drawn dashed and reported.
    written: bool,
    failure: Option<String>,
}

/// The id a title gets when the author names none: lower case, words joined by hyphens.
fn slug(title: &str) -> String {
    let mut out = String::new();
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// `a -> b`, `a ..> b` (dashed) and either with `: label`.
pub(crate) fn parse_edge(text: &str) -> Option<(String, String, Option<String>, bool)> {
    let (arrow, dashed) = if text.contains("..>") {
        ("..>", true)
    } else {
        ("->", false)
    };
    let (from, rest) = text.split_once(arrow)?;
    let (to, label) = match rest.split_once(": ") {
        Some((to, label)) => (to, Some(label.trim().to_owned()).filter(|l| !l.is_empty())),
        None => match rest.trim_end().strip_suffix(':') {
            Some(to) => (to, None),
            None => (rest, None),
        },
    };
    let (from, to) = (from.trim(), to.trim());
    (!from.is_empty() && !to.is_empty()).then(|| (from.to_owned(), to.to_owned(), label, dashed))
}

/// The rows a flow written as `nodes` and `edges` shows in its Table view and CSV: one per card,
/// then one per edge, in the order written (TYP-25).
pub(crate) fn table_rows(block: &Value) -> Option<Vec<Value>> {
    let nodes = block.get("nodes")?.as_array()?;
    let mut rows = Vec::new();
    let groups = block.get("groups").and_then(Value::as_array);
    let mut ids: Vec<(String, String)> = Vec::new();
    for n in nodes {
        let title = n.get("title").and_then(Value::as_str).unwrap_or_default().to_owned();
        let id = n
            .get("id")
            .and_then(Value::as_str)
            .map_or_else(|| slug(&title), str::to_owned);
        ids.push((id, title));
    }
    let title_of = |r: &str| {
        ids.iter()
            .find(|(id, _)| id == r)
            .or_else(|| ids.iter().find(|(_, t)| t == r))
            .map_or_else(|| r.to_owned(), |x| x.1.clone())
    };
    for (n, (id, title)) in nodes.iter().zip(&ids) {
        let group = groups.and_then(|gs| {
            gs.iter().find_map(|g| {
                let members = g.get("nodes")?.as_array()?;
                members
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|m| m == id || m == title)
                    .then(|| g.get("label").and_then(Value::as_str).unwrap_or_default().to_owned())
            })
        });
        let mut row = serde_json::Map::new();
        row.insert("step".into(), json!(title));
        row.insert(
            "kind".into(),
            json!(n.get("kind").and_then(Value::as_str).unwrap_or("step")),
        );
        if let Some(g) = group {
            row.insert("group".into(), json!(g));
        }
        rows.push(Value::Object(row));
    }
    for e in block.get("edges").and_then(Value::as_array).into_iter().flatten() {
        let text = e
            .as_str()
            .or_else(|| e.get("edge").and_then(Value::as_str))
            .unwrap_or_default();
        if let Some((from, to, label, _)) = parse_edge(text) {
            let mut row = serde_json::Map::new();
            row.insert("from".into(), json!(title_of(&from)));
            row.insert("to".into(), json!(title_of(&to)));
            if let Some(l) = label {
                row.insert("label".into(), json!(l));
            }
            rows.push(Value::Object(row));
        }
    }
    Some(rows)
}

fn text_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// The lines under a card's title: a string broken at newlines, or a list of strings.
fn lines_of(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::String(s)) => s
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(text_of)
            .flat_map(|t| {
                t.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A flow written in the block: `nodes`, `edges`, `groups` and `direction`.
fn read_spec(ctx: &mut Ctx, raw: &Value) -> Flow {
    let mut flow = Flow {
        across: raw.get("direction").and_then(Value::as_str) == Some("right"),
        written: true,
        ..Flow::default()
    };
    let nodes = raw.get("nodes").and_then(Value::as_array).cloned().unwrap_or_default();
    for (i, n) in nodes.iter().enumerate() {
        let Some(title) = n.get("title").and_then(text_of) else {
            ctx.check(
                "source-error",
                Level::Error,
                format!("Card {} has no title and is left out", i + 1),
                Some("Give every card a `title`: a verb and an object for a step, a short question for a decision."),
                Some(format!("chart.nodes[{i}].title")),
                None,
                json!({"node": i}),
            );
            continue;
        };
        let id = n.get("id").and_then(text_of).unwrap_or_else(|| slug(&title));
        if flow.nodes.iter().any(|m| m.id == id) {
            ctx.check(
                "source-error",
                Level::Error,
                format!("Card {} repeats the id '{id}'; the first card with it stays", i + 1),
                Some("Give the card its own `id`."),
                Some(format!("chart.nodes[{i}].id")),
                None,
                json!({"node": i}),
            );
            continue;
        }
        let kind = n
            .get("kind")
            .and_then(Value::as_str)
            .and_then(Kind::parse)
            .unwrap_or(Kind::Step);
        flow.nodes.push(Node {
            id,
            title,
            lines: lines_of(n.get("lines")),
            kind,
            tone: n
                .get("tone")
                .and_then(Value::as_str)
                .and_then(Tone::parse)
                .unwrap_or(Tone::Plain),
            group: None,
            lane: None,
            row: Some(i),
        });
    }
    let find = |flow: &Flow, r: &str| {
        flow.nodes
            .iter()
            .position(|n| n.id == r)
            .or_else(|| flow.nodes.iter().position(|n| n.title == r))
            .or_else(|| flow.nodes.iter().position(|n| n.title.eq_ignore_ascii_case(r)))
    };
    for (gi, g) in raw
        .get("groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let label = g.get("label").and_then(text_of).unwrap_or_default();
        let index = flow.groups.len();
        flow.groups.push(Group {
            label,
            tone: g
                .get("tone")
                .and_then(Value::as_str)
                .and_then(Tone::parse)
                .unwrap_or(Tone::Plain),
        });
        for (mi, m) in g
            .get("nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let Some(r) = m.as_str() else { continue };
            match find(&flow, r) {
                Some(k) if flow.nodes[k].group.is_none() => flow.nodes[k].group = Some(index),
                Some(k) => ctx.check(
                    "source-error",
                    Level::Warning,
                    format!(
                        "'{}' is already in another group; a card sits in one group",
                        flow.nodes[k].title
                    ),
                    Some("List the card in one group only."),
                    Some(format!("chart.groups[{gi}].nodes[{mi}]")),
                    None,
                    json!({"group": gi}),
                ),
                None => ctx.check(
                    "source-error",
                    Level::Error,
                    format!("Group {} lists '{r}', which is no card's id or title", gi + 1),
                    Some("Use a card's `id` or its exact title."),
                    Some(format!("chart.groups[{gi}].nodes[{mi}]")),
                    None,
                    json!({"group": gi}),
                ),
            }
        }
    }
    let base = nodes.len();
    for (i, e) in raw
        .get("edges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let (text, tone) = match e {
            Value::String(s) => (s.as_str(), Tone::Plain),
            Value::Object(m) => (
                m.get("edge").and_then(Value::as_str).unwrap_or_default(),
                m.get("tone")
                    .and_then(Value::as_str)
                    .and_then(Tone::parse)
                    .unwrap_or(Tone::Plain),
            ),
            _ => ("", Tone::Plain),
        };
        let path = format!("chart.edges[{i}]");
        let Some((a, b, label, dashed)) = parse_edge(text) else {
            ctx.check(
                "source-error",
                Level::Error,
                format!("Edge {} is not written as `from -> to` or `from ..> to`", i + 1),
                Some("Write an edge as `Clean the orders -> Score the orders: label`."),
                Some(path),
                None,
                json!({"edge": i}),
            );
            continue;
        };
        let (Some(from), Some(to)) = (find(&flow, &a), find(&flow, &b)) else {
            let missing = if find(&flow, &a).is_none() { a } else { b };
            ctx.check(
                "source-error",
                Level::Error,
                format!(
                    "Edge {} joins '{missing}', which is no card's id or title; it is left out",
                    i + 1
                ),
                Some("Use a card's `id` or its exact title."),
                Some(path),
                None,
                json!({"edge": i}),
            );
            continue;
        };
        push_edge(
            ctx,
            &mut flow,
            Edge {
                from,
                to,
                label,
                dashed,
                tone,
                row: Some(base + i),
            },
            path,
        );
    }
    flow
}

fn push_edge(ctx: &mut Ctx, flow: &mut Flow, edge: Edge, path: String) {
    let (a, b) = (&flow.nodes[edge.from].title, &flow.nodes[edge.to].title);
    if edge.from == edge.to {
        ctx.check(
            "source-error",
            Level::Warning,
            format!("'{a}' leads to itself; the edge is left out"),
            Some("Draw the retry as a step of its own, or remove the edge."),
            Some(path),
            None,
            json!({}),
        );
        return;
    }
    if flow.edges.iter().any(|e| e.from == edge.from && e.to == edge.to) {
        ctx.check(
            "source-error",
            Level::Warning,
            format!("'{a}' to '{b}' is written twice; the first one stays"),
            None,
            Some(path),
            None,
            json!({}),
        );
        return;
    }
    flow.edges.push(edge);
}

/// A flow drawn from data rows through `encoding`.
fn read_rows(ctx: &mut Ctx, panel: usize, members: &[usize]) -> Flow {
    let block = ctx.block;
    let label_f = block.encoding.label.clone().unwrap_or_default();
    let source_f = block.channel("source").unwrap_or_default();
    let target_f = block.channel("target").unwrap_or_default();
    let lane_f = block.channel("lane");
    let value_f = block.encoding.y.clone();
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut flow = Flow {
        across: block.raw.get("direction").and_then(Value::as_str) == Some("right"),
        ..Flow::default()
    };
    let mut pending: Vec<(usize, String, String, Option<String>)> = Vec::new();
    let data = ctx.rows;
    // A connection's label is its value written out. Its width sets the room it takes, so the
    // text the exact rows give decides the layout, and the runtime reuses it (TYP-36).
    let texts: Vec<(usize, String)> = ctx.derive(panel, "flow.labels", || {
        let mut out: Vec<(usize, String)> = Vec::new();
        if let Some(f) = value_f.as_deref() {
            for &i in members {
                let text = match rows::num(&data[i], f) {
                    Num::Value(v) => Some(crate::numbers::value(v, &readout)),
                    _ => rows::text(&data[i], f),
                };
                out.extend(text.map(|t| (i, t)));
            }
        }
        out
    });
    for &i in members {
        let row = &data[i];
        let label = rows::text(row, &label_f);
        let (from, to) = (rows::text(row, &source_f), rows::text(row, &target_f));
        if from.is_some() != to.is_some() {
            ctx.check(
                "excluded-observations",
                Level::Warning,
                format!("Row {} names only one end of a connection and is left out", i + 1),
                Some("Give a connection both a source and a target."),
                Some(Ctx::row_path(i, if from.is_some() { &target_f } else { &source_f })),
                None,
                json!({"row": i}),
            );
        }
        if let Some(name) = &label {
            if flow.nodes.iter().any(|n| n.title == *name) {
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!(
                        "Row {} repeats the step '{name}'; every step needs a unique name",
                        i + 1
                    ),
                    Some("Give each step its own name."),
                    Some(Ctx::row_path(i, &label_f)),
                    None,
                    json!({"row": i}),
                );
            } else {
                let lane = lane_f.as_deref().and_then(|f| rows::text(row, f)).map(|l| {
                    flow.lanes.iter().position(|x| *x == l).unwrap_or_else(|| {
                        flow.lanes.push(l);
                        flow.lanes.len() - 1
                    })
                });
                flow.nodes.push(Node {
                    id: name.clone(),
                    title: name.clone(),
                    lines: Vec::new(),
                    kind: Kind::Step,
                    tone: Tone::Plain,
                    group: None,
                    lane,
                    row: Some(i),
                });
            }
        }
        match (from, to) {
            (Some(a), Some(b)) => {
                let text = texts.iter().find(|t| t.0 == i).map(|t| t.1.clone());
                pending.push((i, a, b, text));
            }
            (None, None) if label.is_none() => ctx.check(
                "excluded-observations",
                Level::Warning,
                format!("Row {} has neither a {label_f} nor a connection and is left out", i + 1),
                None,
                Some(Ctx::row_path(i, &label_f)),
                None,
                json!({"row": i}),
            ),
            _ => {}
        }
    }
    if !flow.lanes.is_empty() && flow.nodes.iter().any(|n| n.lane.is_none()) {
        flow.lanes.push("Other".into());
        let other = flow.lanes.len() - 1;
        for n in &mut flow.nodes {
            n.lane.get_or_insert(other);
        }
    }
    for (row, a, b, label) in pending {
        let find = |name: &str| flow.nodes.iter().position(|n| n.title == name);
        let (Some(from), Some(to)) = (find(&a), find(&b)) else {
            let missing = if find(&a).is_none() { &a } else { &b };
            ctx.check(
                "source-error",
                Level::Error,
                format!(
                    "Row {} connects '{missing}', which is not a step; the connection is left out",
                    row + 1
                ),
                Some("Add a row for the step, or fix the name."),
                Some(Ctx::row_path(
                    row,
                    if find(&a).is_none() { &source_f } else { &target_f },
                )),
                None,
                json!({"row": row}),
            );
            continue;
        };
        push_edge(
            ctx,
            &mut flow,
            Edge {
                from,
                to,
                label,
                dashed: false,
                tone: Tone::Plain,
                row: Some(row),
            },
            Ctx::row_path(row, &target_f),
        );
    }
    flow
}

/// How the cards' text is set: the faces, sizes and inner spacing.
#[derive(Clone)]
struct Type {
    font: Font,
    face: Face,
    title: f64,
    body: f64,
    small: f64,
    /// Bold text runs this much wider than the regular face the metrics hold.
    bold: f64,
    /// Space between a card's side and its text.
    pad_x: f64,
    /// The style's sizes, which the floors of a shrunk flow are counted from.
    base: f64,
}

impl Type {
    fn new(style: &Style) -> Type {
        // A flow sets prose: a style whose chart face is monospaced, for figures, lends the cards
        // its interface face instead.
        let mono = (style.font.width("i", 10.0) - style.font.width("m", 10.0)).abs() < 0.01;
        let (font, face) = if mono {
            (style.ui.clone(), Face::Ui)
        } else {
            (style.font.clone(), Face::Data)
        };
        let s = style.size;
        Type {
            font,
            face,
            title: s + 1.0,
            body: s,
            small: s - 1.0,
            bold: 1.06,
            pad_x: PAD_X,
            base: s,
        }
    }
    /// The type for a drawing shrunk by `k`: each size is set larger, so that on the page a
    /// title is never under the style's size less 1 px and no other text under it less 2 px,
    /// up to half as large again.
    fn held(&self, k: f64) -> Type {
        let s = self.base;
        let hold = |size: f64, floor: f64| size.max(floor / k).min(size * TYPE_HOLD);
        Type {
            title: hold(s + 1.0, s - 1.0),
            body: hold(s, s - 2.0),
            small: hold(s - 1.0, s - 2.0),
            ..self.clone()
        }
    }
    fn lh(&self, size: f64) -> f64 {
        self.font.line_height(size)
    }
    fn width(&self, text: &str, size: f64, bold: bool) -> f64 {
        self.font.width(text, size) * if bold { self.bold } else { 1.0 }
    }
    fn wrap(&self, text: &str, size: f64, width: f64, bold: bool) -> Vec<String> {
        let w = width / if bold { self.bold } else { 1.0 };
        self.font.wrap(text, size, w, usize::MAX)
    }
}

const PAD_X: f64 = 10.0;
const PAD_Y: f64 = 8.0;
const LINES_GAP: f64 = 2.0;
/// A decision is 2.5 times as wide as it is tall, its text 12 px clear of the outline.
const DIAMOND_RATIO: f64 = 2.5;
const DIAMOND_PAD: f64 = 12.0;
const CAP: f64 = 5.0;
const FOLD: f64 = 9.0;
const ARROW: f64 = 7.0;

/// A card's text, wrapped, and its outer size in page axes.
struct Card {
    title: Vec<String>,
    lines: Vec<String>,
    w: f64,
    h: f64,
}

fn measure(t: &Type, node: &Node, max: f64, tight: bool) -> Card {
    let (lt, lb) = (t.lh(t.title), t.lh(t.body));
    match node.kind {
        Kind::Decision => {
            // A text box tw × th fits a diamond of half-width a and half-height a / R when
            // tw / 2 + R · th / 2 ≤ a; 12 px clear of the outline adds 12 · √(1 + R²). The
            // question takes the wrap that gives the smallest diamond, three lines at most, and
            // a line more only when it saves real width; `tight`, for a flow too wide for its
            // page, takes the line more whenever it saves any.
            let r = DIAMOND_RATIO;
            let clear = DIAMOND_PAD * (1.0 + r * r).sqrt();
            let text = |wrap: f64| {
                let title = t.wrap(&node.title, t.title, wrap, true);
                let lines: Vec<String> = node.lines.iter().flat_map(|l| t.wrap(l, t.body, wrap, false)).collect();
                let tw = title
                    .iter()
                    .map(|l| t.width(l, t.title, true))
                    .chain(lines.iter().map(|l| t.width(l, t.body, false)))
                    .fold(0.0, f64::max);
                let th = title.len() as f64 * lt
                    + if lines.is_empty() {
                        0.0
                    } else {
                        LINES_GAP + lines.len() as f64 * lb
                    };
                let a = tw / 2.0 + r * th / 2.0 + clear;
                (title, lines, a)
            };
            let mut best = text(max.max(120.0));
            let mut wrap = 72.0;
            while wrap < max.max(120.0) {
                let next = text(wrap);
                let (rows, best_rows) = (next.0.len(), best.0.len());
                let line = if tight { 0.0 } else { 6.0 };
                if rows <= 3 && next.2 + line * (rows as f64) < best.2 + line * best_rows as f64 {
                    best = next;
                }
                wrap += 8.0;
            }
            let (title, lines, a) = best;
            let w = step_up(2.0 * a);
            Card {
                title,
                lines,
                w,
                h: w / r,
            }
        }
        Kind::Start | Kind::End => {
            let title = t.wrap(&node.title, t.title, max.max(160.0), true);
            let tw = title.iter().map(|l| t.width(l, t.title, true)).fold(0.0, f64::max);
            let h = title.len() as f64 * lt + 2.0 * 7.0;
            Card {
                title,
                lines: Vec::new(),
                w: step_up((tw + h + 4.0).max(h * 2.0)),
                h,
            }
        }
        _ => {
            // A title takes three lines at most: where narrowing would need a fourth, the card
            // stops narrowing, and the flow shrinks instead.
            let mut inner = max - 2.0 * t.pad_x;
            let mut title = t.wrap(&node.title, t.title, inner, true);
            while title.len() > 3 && inner < CARD_WIDTHS[0] {
                inner += 8.0;
                title = t.wrap(&node.title, t.title, inner, true);
            }
            let lines: Vec<String> = node
                .lines
                .iter()
                .flat_map(|l| t.wrap(l, t.body, inner, false))
                .collect();
            let tw = title
                .iter()
                .map(|l| t.width(l, t.title, true))
                .chain(lines.iter().map(|l| t.width(l, t.body, false)))
                .fold(0.0, f64::max);
            let mut h = 2.0 * PAD_Y + title.len() as f64 * lt;
            if !lines.is_empty() {
                h += LINES_GAP + lines.len() as f64 * lb;
            }
            let mut w = (tw + 2.0 * t.pad_x).max(72.0);
            if node.kind == Kind::Store {
                h += 2.0 * CAP;
            }
            if node.kind == Kind::Object {
                w += FOLD * 0.5;
            }
            Card {
                title,
                lines,
                w: step_up(w).min(max.max(w)),
                h,
            }
        }
    }
}

/// Card widths come in steps, so an edit to a card's text seldom changes its width, and an edit
/// that keeps the width moves no other card (FEA-CH-76).
const WIDTH_STEP: f64 = 16.0;

fn step_up(w: f64) -> f64 {
    (w / WIDTH_STEP).ceil() * WIDTH_STEP
}

struct Laid {
    flow: Flow,
    cards: Vec<Card>,
    engine: engine::Laid,
    across: bool,
    /// The type the cards were measured with.
    t: Type,
    /// Edge labels in page axes, by edge.
    labels: Vec<Option<Rect>>,
    groups: Vec<Rect>,
    group_labels: Vec<Option<Rect>>,
    lanes: Vec<Rect>,
    width: f64,
    height: f64,
}

fn params(t: &Type, step: Step) -> Params {
    Params {
        node_gap: if step.dense { 12.0 } else { 22.0 },
        rank_gap: 30.0,
        track_gap: 8.0,
        group_pad: if step.dense { 8.0 } else { 12.0 },
        lane_pad: if step.dense { 6.0 } else { 10.0 },
        arrow: ARROW,
        label_pad: 3.0,
        label_clear: LABEL_CLEAR,
        lane_label: [0.0, t.lh(t.small) + 4.0],
        stack_lanes: step.stack,
        snug: step.narrow(),
    }
}

/// One way to lay a flow out, from roomy to tight: how wide a card may be, whether a question
/// wraps to its narrowest diamond, whether the paper between and inside cards is tight, and
/// whether a lane is one column.
#[derive(Debug, Clone, Copy)]
struct Step {
    max: f64,
    tight: bool,
    dense: bool,
    stack: bool,
}

impl Step {
    /// The cards are at their narrowest step: the paper is tight, and the cards take what room
    /// the page has.
    fn narrow(self) -> bool {
        self.max <= CARD_WIDTHS[CARD_WIDTHS.len() - 1]
    }
}

/// Least space between an edge label and a card on the page (FEA-CH-76).
const LABEL_CLEAR: f64 = 4.5;

fn label_size(t: &Type, text: &str) -> [f64; 2] {
    [t.width(text, t.small, false) + 4.0, t.lh(t.small)]
}

fn group_label_size(t: &Type, text: &str) -> [f64; 2] {
    [t.width(text, t.small, true) + 8.0, t.lh(t.small)]
}

/// Lay the flow out by `step`, in `across` or down the page. `wider` gives single cards more
/// width than the step's.
fn lay(t: &Type, flow: Flow, step: Step, across: bool, label_clear: f64, wider: &[f64]) -> Laid {
    let t = &Type {
        pad_x: if step.dense { 7.0 } else { PAD_X },
        ..t.clone()
    };
    let cards: Vec<Card> = flow
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            measure(
                t,
                n,
                wider.get(i).copied().unwrap_or(step.max).max(step.max),
                step.tight,
            )
        })
        .collect();
    let swap = |s: [f64; 2]| if across { [s[1], s[0]] } else { s };
    let mut p = params(t, step);
    // Across the page a lane is a row already.
    p.stack_lanes = step.stack && !across;
    p.label_clear = label_clear;
    if across {
        // Across the page a lane is a row, its name at the start of the row's top edge.
        p.lane_label = [t.lh(t.small) + 4.0, 0.0];
    }
    let input = Input {
        nodes: flow
            .nodes
            .iter()
            .zip(&cards)
            .map(|(n, c)| {
                let [b, d] = swap([c.w, c.h]);
                NodeIn {
                    breadth: b,
                    depth: d,
                    group: n.group,
                    lane: n.lane,
                    ports: if n.kind == Kind::Decision {
                        Ports::Corners
                    } else {
                        Ports::Side
                    },
                }
            })
            .collect(),
        edges: flow
            .edges
            .iter()
            .map(|e| EdgeIn {
                from: e.from,
                to: e.to,
                label: e.label.as_deref().map(|l| swap(label_size(t, l))),
                alert: e.tone == Tone::Alert || flow.nodes[e.to].tone == Tone::Alert,
            })
            .collect(),
        groups: flow
            .groups
            .iter()
            .map(|g| GroupIn {
                label: if across {
                    [0.0, 0.0]
                } else {
                    group_label_size(t, &g.label)
                },
            })
            .collect(),
        lanes: flow.lanes.len(),
        params: p,
    };
    let laid = engine::layout(&input);
    let page = |r: Rect| if across { Rect::new(r.y, r.x, r.h, r.w) } else { r };
    let labels: Vec<Option<Rect>> = laid.edges.iter().map(|e| e.label.map(page)).collect();
    let groups: Vec<Rect> = laid.groups.iter().copied().map(page).collect();
    let lanes: Vec<Rect> = laid.lanes.iter().copied().map(page).collect();
    let [w, h] = swap(laid.extent);
    let mut out = Laid {
        group_labels: laid
            .group_labels
            .iter()
            .map(|r| (r.w > 0.0 && !across).then_some(*r))
            .collect(),
        flow,
        cards,
        engine: laid,
        across,
        t: t.clone(),
        labels,
        groups,
        lanes,
        width: w,
        height: h,
    };
    if across {
        place_group_labels_across(t, &mut out);
    }
    out
}

/// Across the page, a group's label sits in its top border, clear of the lines into the group.
fn place_group_labels_across(t: &Type, laid: &mut Laid) {
    let routes: Vec<Vec<[f64; 2]>> = laid
        .engine
        .edges
        .iter()
        .map(|e| e.points.iter().map(|p| [p[1], p[0]]).collect())
        .collect();
    for (gi, b) in laid.groups.clone().iter().enumerate() {
        if b.w <= 0.0 {
            laid.group_labels.push(None);
            continue;
        }
        let [lw, lh] = group_label_size(t, &laid.flow.groups[gi].label);
        let mut placed = None;
        let mut x = b.x + 10.0;
        while x + lw <= b.right() - 6.0 {
            let r = Rect::new(x, b.y - lh / 2.0, lw, lh);
            let hit = routes
                .iter()
                .any(|pts| pts.windows(2).any(|s| engine::seg_hits(s[0], s[1], &r, 2.0)));
            if !hit {
                placed = Some(r);
                break;
            }
            x += 6.0;
        }
        if laid.group_labels.len() > gi {
            laid.group_labels[gi] = placed;
        } else {
            laid.group_labels.push(placed);
        }
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let width = ctx.opts.width;
    let spec = ctx.block.raw.get("nodes").is_some();
    let mut flows: Vec<Flow> = if spec {
        let raw = ctx.block.raw.clone();
        vec![read_spec(ctx, &raw)]
    } else {
        facets
            .iter()
            .enumerate()
            .map(|(fi, (_, members))| read_rows(ctx, fi, members))
            .collect()
    };
    for flow in flows.iter_mut() {
        let path = if spec { "chart.nodes" } else { "chart.encoding.label" };
        if flow.nodes.len() > limits::max("flow", "steps") {
            flow.failure = Some(kit::over_limit(ctx, "flow", "steps", flow.nodes.len(), Some(path)));
        } else if flow.edges.len() > limits::max("flow", "connections") {
            flow.failure = Some(kit::over_limit(
                ctx,
                "flow",
                "connections",
                flow.edges.len(),
                Some(path),
            ));
        } else if flow.nodes.is_empty() {
            flow.failure = Some(if spec {
                "Nothing to draw · No card has a title".to_owned()
            } else {
                "Nothing to draw · No row names a step".to_owned()
            });
        }
    }
    if flows.iter().all(|f| f.failure.is_some()) {
        let reason = flows[0].failure.clone().unwrap_or_default();
        let h = width / 2.0;
        let panel = super::failed_panel(ctx, Rect::new(0.0, 0.0, width, h), &reason);
        return Painted {
            height: h,
            panels: vec![panel],
            figure_items: vec![],
            legend: None,
            failure: Some(reason),
        };
    }
    // Groups and lanes are the colours a reader tells apart; their names sit in the drawing.
    let mut keys: Vec<String> = Vec::new();
    for f in &flows {
        for g in &f.groups {
            if !keys.contains(&g.label) {
                keys.push(g.label.clone());
            }
        }
        for l in &f.lanes {
            if !keys.contains(l) {
                keys.push(l.clone());
            }
        }
    }
    if keys.is_empty() {
        keys.push("Steps".into());
    }
    ctx.set_series(&keys);
    let t = Type::new(&ctx.style);
    let title_room = if ctx.block.encoding.facet.is_some() && !spec {
        ctx.line_height() + 6.0
    } else {
        0.0
    };
    let names: Vec<Option<String>> = if spec {
        vec![None]
    } else {
        facets.iter().map(|(n, _)| n.clone()).collect()
    };
    let mut panels = Vec::new();
    let mut cursor = 0.0;
    let (mut steps, mut links) = (0usize, 0usize);
    let mut described: Vec<String> = Vec::new();
    for (fi, flow) in flows.into_iter().enumerate() {
        if fi > 0 {
            cursor += super::FACET_GAP_Y;
        }
        if let Some(reason) = &flow.failure {
            let frame = Rect::new(0.0, cursor, width, 120.0 + title_room);
            panels.push(super::failed_facet(ctx, frame, title_room, &names[fi], reason));
            cursor += frame.h;
            continue;
        }
        let room = (width - 2.0 * EDGE_CLEAR).max(1.0);
        let laid = fit(&t, flow, room);
        if !laid.flow.written {
            for (ei, e) in laid.flow.edges.iter().enumerate() {
                if laid.engine.edges[ei].back {
                    let (a, b) = (&laid.flow.nodes[e.from].title, &laid.flow.nodes[e.to].title);
                    ctx.check(
                        "cycle",
                        Level::Warning,
                        format!("'{a}' to '{b}' closes a cycle; it is drawn dashed as a return to an earlier step"),
                        Some("Remove the connection if the flow should end."),
                        e.row
                            .map(|r| Ctx::row_path(r, &ctx.block.channel("target").unwrap_or_default())),
                        None,
                        json!({"row": e.row}),
                    );
                }
            }
        }
        let top = cursor + title_room + EDGE_CLEAR;
        let k = (room / laid.width.max(1.0)).min(1.0);
        let left = EDGE_CLEAR + ((room - laid.width * k) / 2.0).max(0.0);
        let mut panel = paint(ctx, &laid.t, &laid, left, top);
        if k < 1.0 {
            scale_panel(&mut panel, k, [left, top]);
        }
        let height = laid.height * k;
        let frame = Rect::new(0.0, cursor, width, height + title_room + 2.0 * EDGE_CLEAR);
        panel.frame = frame;
        panel.plot = Rect::new(0.0, cursor + title_room, width, height + 2.0 * EDGE_CLEAR);
        super::panel_title(ctx, &mut panel, &names[fi]);
        panels.push(panel);
        cursor += frame.h;
        steps += laid.flow.nodes.len();
        links += laid.flow.edges.len();
        described.extend(
            laid.flow
                .edges
                .iter()
                .map(|e| format!("{} to {}", laid.flow.nodes[e.from].title, laid.flow.nodes[e.to].title)),
        );
    }
    let mut summary = format!(
        "Flow chart of {} and {}",
        super::count(steps, "step", "steps"),
        super::count(links, "connection", "connections")
    );
    if described.is_empty() {
        summary.push('.');
    } else {
        let listed: Vec<&str> = described.iter().take(12).map(String::as_str).collect();
        summary.push_str(&format!(": {}", listed.join(", ")));
        if described.len() > 12 {
            summary.push_str(&format!(", and {} more", described.len() - 12));
        }
        summary.push('.');
    }
    ctx.summary = summary;
    Painted {
        height: cursor,
        panels,
        figure_items: vec![],
        legend: None,
        failure: None,
    }
}

/// Widest card the layout tries first, and the narrowest it wraps to before it scales.
const CARD_WIDTHS: [f64; 5] = [280.0, 230.0, 190.0, 160.0, 130.0];

/// Nothing draws on the chart's edge: the drawing keeps this far from it on every side.
const EDGE_CLEAR: f64 = 4.0;

/// The narrowest a card wraps to when the paper is tight.
const DENSE_WIDTH: f64 = 96.0;
/// How much larger than the style's sizes the type of a shrunk flow may be set.
const TYPE_HOLD: f64 = 1.5;

/// The roomiest layout that fits `width`. Paper gives way before type: cards wrap narrower, then
/// questions take a line more, then a flow across the page turns down it, then the paper between
/// and inside the cards is tight, then a lane is one column. A flow that still does not fit is
/// the narrowest of these, which the caller scales, its type set larger by as much, so that on
/// the page it holds its floor ([`Type::held`]).
fn fit(t: &Type, flow: Flow, width: f64) -> Laid {
    let narrowest = CARD_WIDTHS[CARD_WIDTHS.len() - 1];
    let step = |max: f64, tight: bool, dense: bool, stack: bool| Step {
        max,
        tight,
        dense,
        stack,
    };
    let roomy: Vec<Step> = CARD_WIDTHS
        .into_iter()
        .map(|w| step(w, false, false, false))
        .chain([step(narrowest, true, false, false)])
        .collect();
    let lanes = !flow.lanes.is_empty();
    let dense: Vec<Step> = [false, true]
        .into_iter()
        .filter(|stack| !stack || lanes)
        .flat_map(|stack| [step(narrowest, true, true, stack), step(DENSE_WIDTH, true, true, stack)])
        .collect();
    let mut best: Option<(Laid, Step, bool)> = None;
    let directions: Vec<bool> = if flow.across { vec![true, false] } else { vec![false] };
    for (i, across) in directions.iter().copied().enumerate() {
        // A flow that turns down is scaled as it runs down, never across; only the last
        // direction goes tight.
        best = None;
        let last = i + 1 == directions.len();
        let ladder: Vec<Step> = roomy.iter().chain(dense.iter().filter(|_| last)).copied().collect();
        for (n, step) in ladder.iter().copied().enumerate() {
            let laid = lay(t, flow.clone(), step, across, LABEL_CLEAR, &[]);
            if laid.width <= width {
                let mut chosen = widened(t, laid, step, LABEL_CLEAR, width);
                if step.narrow() {
                    // With the cards at their narrowest, paper gives way before a card takes a
                    // line more: of the ways to lay them that fit, the one whose cards take
                    // the fewest lines, and of those the roomiest.
                    let rows = |laid: &Laid| {
                        laid.flow
                            .nodes
                            .iter()
                            .zip(&laid.cards)
                            .filter(|(n, _)| matches!(n.kind, Kind::Step | Kind::Store | Kind::Object))
                            .map(|(_, c)| c.title.len() + c.lines.len())
                            .sum::<usize>()
                    };
                    for other in ladder
                        .iter()
                        .skip(n + 1)
                        .filter(|o| o.narrow() && o.stack == step.stack)
                    {
                        let laid = lay(t, flow.clone(), *other, across, LABEL_CLEAR, &[]);
                        if laid.width <= width {
                            let laid = widened(t, laid, *other, LABEL_CLEAR, width);
                            if rows(&laid) < rows(&chosen) {
                                chosen = laid;
                            }
                        }
                    }
                }
                return chosen;
            }
            if best.as_ref().is_none_or(|b| laid.width < b.0.width) {
                best = Some((laid, step, across));
            }
        }
    }
    let (mut laid, step, across) = best.expect("one layout");
    // Shrunk to fit, the type holds its floor and a label keeps its space from the cards on the
    // page: both are set larger by the scale. Larger type widens the cards, so the scale is
    // found in a few rounds.
    let mut k = width / laid.width.max(1.0);
    for _ in 0..6 {
        let held = t.held(k);
        laid = lay(&held, flow.clone(), step, across, LABEL_CLEAR / k, &[]);
        laid = widened(&held, laid, step, LABEL_CLEAR / k, 0.0);
        let next = width / laid.width.max(1.0);
        if next >= k * 0.995 {
            break;
        }
        k = next;
    }
    laid
}

/// Cards take the room the drawing already has. On tight paper the cards that share a rank
/// share the page's room: a rank is as tall as its tallest card, so the card with the most
/// lines wraps wider, by the least that saves it a line, as long as the drawing fits the
/// page, and a card with fewer lines than a neighbour keeps its width. Then a card is as wide
/// as its column already is: where a question, a pill or another card of its column is wider
/// than the step lets a card be, the column's cards that stand alone in their rank wrap to
/// that width, column by column. `room` is the page's width for a drawing that fits it; a
/// drawing that is scaled to fit grows no wider than it is. A flow across the page has no
/// columns, and lanes keep theirs.
fn widened(t: &Type, first: Laid, step: Step, label_clear: f64, room: f64) -> Laid {
    if first.across || step.max >= CARD_WIDTHS[0] {
        return first;
    }
    // The widest the drawing may become: the page where the paper is tight, else what it is.
    let bound = |laid: &Laid| if step.narrow() { laid.width.max(room) } else { laid.width } + 0.5;
    let n = first.flow.nodes.len();
    let card = |laid: &Laid, i: usize| matches!(laid.flow.nodes[i].kind, Kind::Step | Kind::Store | Kind::Object);
    let rows = |card: &Card| card.title.len() + card.lines.len();
    let mut wider = vec![step.max; n];
    let mut laid = first;
    let mut shared = vec![false; n];
    if step.narrow() && laid.flow.lanes.is_empty() {
        // The type the cards are measured with, as `lay` sets it.
        let measured = Type {
            pad_x: if step.dense { 7.0 } else { PAD_X },
            ..t.clone()
        };
        let mut ranks = laid.engine.ranks[..n].to_vec();
        ranks.sort_unstable();
        ranks.dedup();
        for rank in ranks {
            let mates: Vec<usize> = (0..n)
                .filter(|&i| laid.engine.ranks[i] == rank && card(&laid, i))
                .collect();
            if mates.len() < 2 {
                continue;
            }
            let mut stuck = vec![false; n];
            for &i in &mates {
                shared[i] = true;
            }
            loop {
                // Of the rank's tallest cards, the narrowest that has not been tried in vain.
                let tallest = mates.iter().map(|&i| rows(&laid.cards[i])).max().unwrap_or(0);
                let Some(i) = mates
                    .iter()
                    .copied()
                    .filter(|&i| !stuck[i] && rows(&laid.cards[i]) == tallest)
                    .min_by(|&a, &b| laid.engine.nodes[a].w.total_cmp(&laid.engine.nodes[b].w))
                else {
                    break;
                };
                // The least width that saves it a line, if the drawing then fits.
                let mut to = wider[i] + WIDTH_STEP;
                while to <= CARD_WIDTHS[0] && rows(&measure(&measured, &laid.flow.nodes[i], to, step.tight)) >= tallest
                {
                    to += WIDTH_STEP;
                }
                let mut trial = wider.clone();
                trial[i] = to;
                let next = (to <= CARD_WIDTHS[0]).then(|| lay(t, laid.flow.clone(), step, false, label_clear, &trial));
                match next {
                    Some(next) if next.width <= bound(&laid) => {
                        laid = next;
                        wider = trial;
                    }
                    _ => stuck[i] = true,
                }
            }
        }
    }
    let centre = |laid: &Laid, i: usize| laid.engine.nodes[i].x + laid.engine.nodes[i].w / 2.0;
    let mut columns: Vec<f64> = (0..n).map(|i| centre(&laid, i)).collect();
    columns.sort_by(f64::total_cmp);
    columns.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    for column in columns {
        let members: Vec<usize> = (0..n).filter(|&i| (centre(&laid, i) - column).abs() < 0.5).collect();
        let widest = members
            .iter()
            .map(|&i| laid.engine.nodes[i].w)
            .fold(0.0, f64::max)
            .min(CARD_WIDTHS[0]);
        let cards: Vec<usize> = members
            .into_iter()
            .filter(|&i| card(&laid, i) && !shared[i])
            .filter(|&i| laid.engine.nodes[i].w < widest - 0.5)
            .collect();
        if cards.is_empty() || widest <= step.max {
            continue;
        }
        let mut trial = wider.clone();
        for &i in &cards {
            trial[i] = widest;
        }
        let next = lay(t, laid.flow.clone(), step, false, label_clear, &trial);
        if next.width <= bound(&laid) {
            laid = next;
            wider = trial;
        }
    }
    laid
}

/// Shrink a painted panel about `origin` by `k`: positions, sizes and type alike, so a flow too
/// wide for a phone keeps its layout.
fn scale_panel(panel: &mut Panel, k: f64, origin: [f64; 2]) {
    let pt = |p: [f64; 2]| [origin[0] + (p[0] - origin[0]) * k, origin[1] + (p[1] - origin[1]) * k];
    let rect = |r: Rect| {
        let [x, y] = pt([r.x, r.y]);
        Rect::new(x, y, r.w * k, r.h * k)
    };
    let path = |p: &mut Path| {
        for c in &mut p.cmds {
            match c {
                Cmd::M { p } | Cmd::L { p } => *p = pt(*p),
                Cmd::C { c1, c2, p } => {
                    *c1 = pt(*c1);
                    *c2 = pt(*c2);
                    *p = pt(*p);
                }
                Cmd::Z => {}
            }
        }
    };
    for item in &mut panel.items {
        match item {
            Item::Rect { rect: r, radius, .. } => {
                *r = rect(*r);
                for v in radius.iter_mut() {
                    *v *= k;
                }
            }
            Item::Line { path: p, .. } | Item::Area { path: p, .. } => path(p),
            Item::Point { center, radius, .. } => {
                *center = pt(*center);
                *radius *= k;
            }
            Item::Text(t) => {
                [t.x, t.y] = pt([t.x, t.y]);
                t.size *= k;
                t.bounds = rect(t.bounds);
            }
        }
    }
    for o in &mut panel.observations {
        o.at = pt(o.at);
        o.x = o.at[0];
        o.area = o.area.map(rect);
    }
}

fn mix(a: Oklch, b: Oklch, t: f64) -> Oklch {
    color::mix(a, b, t)
}

/// The colours of one card or line: fill, outline and title ink.
struct Paint {
    fill: Oklch,
    line: Oklch,
    ink: Oklch,
    soft: Oklch,
}

fn tone_color(ctx: &Ctx, tone: Tone) -> Option<Oklch> {
    match tone {
        Tone::Alert => Some(ctx.style.alert),
        Tone::Muted => Some(ctx.style.ink_faint),
        Tone::Plain => None,
    }
}

fn card_paint(ctx: &Ctx, flow: &Flow, node: &Node) -> Paint {
    let s = &ctx.style;
    let group_tone = node.group.map(|g| flow.groups[g].tone).unwrap_or(Tone::Plain);
    let hue = tone_color(ctx, node.tone)
        .or_else(|| tone_color(ctx, group_tone))
        .or_else(|| {
            node.group.map(|g| {
                ctx.series_color(
                    ctx.series
                        .iter()
                        .position(|x| x.key == flow.groups[g].label)
                        .unwrap_or(0),
                )
            })
        })
        .or_else(|| {
            node.lane
                .map(|l| ctx.series_color(ctx.series.iter().position(|x| x.key == flow.lanes[l]).unwrap_or(0)))
        });
    match hue {
        Some(c) if node.tone != Tone::Muted && group_tone != Tone::Muted => Paint {
            fill: mix(s.surface, c, 0.11),
            line: mix(s.surface, c, 0.85),
            ink: s.ink,
            soft: s.ink_soft,
        },
        _ => Paint {
            fill: mix(s.surface, s.ink, 0.045),
            line: mix(s.surface, s.ink, 0.42),
            ink: s.ink,
            soft: s.ink_soft,
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn text(
    t: &Type,
    s: &str,
    size: f64,
    weight: u16,
    color: Oklch,
    x: f64,
    baseline: f64,
    anchor: Anchor,
    role: Role,
) -> Text {
    let mut bounds = t.font.bounds(s, size, x, baseline, anchor);
    if weight >= 600 {
        let grow = bounds.w * (t.bold - 1.0);
        bounds.w += grow;
        match anchor {
            Anchor::Middle => bounds.x -= grow / 2.0,
            Anchor::End => bounds.x -= grow,
            Anchor::Start => {}
        }
    }
    Text {
        text: s.to_owned(),
        x,
        y: baseline,
        anchor,
        face: t.face,
        size,
        weight,
        color: Style::rgba(color),
        rotate: 0.0,
        bounds,
        role,
        series: None,
        mark: None,
        near: false,
    }
}

/// An orthogonal line with its corners rounded.
fn rounded(points: &[[f64; 2]], radius: f64) -> Path {
    let mut path = Path::new();
    if points.is_empty() {
        return path;
    }
    path.move_to(points[0]);
    for i in 1..points.len() {
        let p = points[i];
        if i + 1 < points.len() {
            let (a, n) = (points[i - 1], points[i + 1]);
            let din = ((p[0] - a[0]).hypot(p[1] - a[1])).max(1e-9);
            let dout = ((n[0] - p[0]).hypot(n[1] - p[1])).max(1e-9);
            let r = radius.min(din / 2.0).min(dout / 2.0);
            let s = [p[0] - (p[0] - a[0]) / din * r, p[1] - (p[1] - a[1]) / din * r];
            let e = [p[0] + (n[0] - p[0]) / dout * r, p[1] + (n[1] - p[1]) / dout * r];
            path.line_to(s);
            let k = 0.552;
            path.curve_to(
                [s[0] + (p[0] - s[0]) * k, s[1] + (p[1] - s[1]) * k],
                [e[0] + (p[0] - e[0]) * k, e[1] + (p[1] - e[1]) * k],
                e,
            );
        } else {
            path.line_to(p);
        }
    }
    path
}

fn arrowhead(tip: [f64; 2], from: [f64; 2]) -> Path {
    let (dx, dy) = (tip[0] - from[0], tip[1] - from[1]);
    let len = dx.hypot(dy).max(1e-9);
    let (ux, uy) = (dx / len, dy / len);
    let (bx, by) = (tip[0] - ux * ARROW, tip[1] - uy * ARROW);
    let half = ARROW * 0.5;
    let mut p = Path::new();
    p.move_to(tip);
    p.line_to([bx - uy * half, by + ux * half]);
    p.line_to([bx + uy * half, by - ux * half]);
    p.close();
    p
}

/// The outline of a card of `kind` in `r`.
fn outline(kind: Kind, r: Rect, radius: f64) -> Path {
    let mut p = Path::new();
    match kind {
        Kind::Decision => {
            let [cx, cy] = r.center();
            p.move_to([cx, r.y]);
            p.line_to([r.right(), cy]);
            p.line_to([cx, r.bottom()]);
            p.line_to([r.x, cy]);
            p.close();
        }
        Kind::Object => {
            let f = FOLD;
            p.move_to([r.x + radius, r.y]);
            p.line_to([r.right() - f, r.y]);
            p.line_to([r.right(), r.y + f]);
            p.line_to([r.right(), r.bottom() - radius]);
            corner(&mut p, [r.right(), r.bottom()], [r.right() - radius, r.bottom()]);
            p.line_to([r.x + radius, r.bottom()]);
            corner(&mut p, [r.x, r.bottom()], [r.x, r.bottom() - radius]);
            p.line_to([r.x, r.y + radius]);
            corner(&mut p, [r.x, r.y], [r.x + radius, r.y]);
            p.close();
        }
        Kind::Store => {
            let k = 0.552;
            let (rx, ry) = (r.w / 2.0, CAP);
            let cx = r.x + rx;
            p.move_to([r.x, r.y + ry]);
            p.curve_to([r.x, r.y + ry - ry * k], [cx - rx * k, r.y], [cx, r.y]);
            p.curve_to(
                [cx + rx * k, r.y],
                [r.right(), r.y + ry - ry * k],
                [r.right(), r.y + ry],
            );
            p.line_to([r.right(), r.bottom() - ry]);
            p.curve_to(
                [r.right(), r.bottom() - ry + ry * k],
                [cx + rx * k, r.bottom()],
                [cx, r.bottom()],
            );
            p.curve_to(
                [cx - rx * k, r.bottom()],
                [r.x, r.bottom() - ry + ry * k],
                [r.x, r.bottom() - ry],
            );
            p.close();
        }
        _ => {
            let rad = match kind {
                Kind::Start | Kind::End => r.h / 2.0,
                _ => radius,
            }
            .min(r.h / 2.0)
            .min(r.w / 2.0);
            p.move_to([r.x + rad, r.y]);
            p.line_to([r.right() - rad, r.y]);
            corner(&mut p, [r.right(), r.y], [r.right(), r.y + rad]);
            p.line_to([r.right(), r.bottom() - rad]);
            corner(&mut p, [r.right(), r.bottom()], [r.right() - rad, r.bottom()]);
            p.line_to([r.x + rad, r.bottom()]);
            corner(&mut p, [r.x, r.bottom()], [r.x, r.bottom() - rad]);
            p.line_to([r.x, r.y + rad]);
            corner(&mut p, [r.x, r.y], [r.x + rad, r.y]);
            p.close();
        }
    }
    p
}

/// A quarter round from the current point, about `at`, to `to`.
fn corner(p: &mut Path, at: [f64; 2], to: [f64; 2]) {
    let from = match p.cmds.last() {
        Some(Cmd::M { p } | Cmd::L { p } | Cmd::C { p, .. }) => *p,
        _ => at,
    };
    if (from[0] - to[0]).abs() < 1e-9 && (from[1] - to[1]).abs() < 1e-9 {
        return;
    }
    let k = 0.552;
    p.curve_to(
        [from[0] + (at[0] - from[0]) * k, from[1] + (at[1] - from[1]) * k],
        [to[0] + (at[0] - to[0]) * k, to[1] + (at[1] - to[1]) * k],
        to,
    );
}

fn shift(r: Rect, dx: f64, dy: f64) -> Rect {
    Rect::new(r.x + dx, r.y + dy, r.w, r.h)
}

#[allow(clippy::too_many_lines)]
fn paint(ctx: &mut Ctx, t: &Type, laid: &Laid, left: f64, top: f64) -> Panel {
    let s = ctx.style.clone();
    let flow = &laid.flow;
    let page = |p: [f64; 2]| {
        if laid.across {
            [p[1] + left, p[0] + top]
        } else {
            [p[0] + left, p[1] + top]
        }
    };
    let node_rect = |i: usize| {
        let r = laid.engine.nodes[i];
        let r = if laid.across { Rect::new(r.y, r.x, r.h, r.w) } else { r };
        shift(r, left, top)
    };
    let radius = (s.bar_radius * 1.6).min(8.0);
    let mut items: Vec<Item> = Vec::new();
    let mut labels: Vec<Item> = Vec::new();
    let mut observations: Vec<Observation> = Vec::new();
    // Lanes: bands with their names at the start.
    for (li, band) in laid.lanes.iter().enumerate() {
        if band.w <= 0.0 {
            continue;
        }
        let slot = ctx.series.iter().position(|x| x.key == flow.lanes[li]).unwrap_or(0);
        let c = ctx.series_color(slot);
        let b = shift(*band, left, top);
        items.push(Item::Rect {
            rect: b,
            fill: Fill::solid(Style::rgba(mix(s.surface, c, 0.06))),
            stroke: None,
            radius: [radius.min(4.0); 4],
            role: Role::Band,
            series: Some(slot),
            obs: None,
        });
        let name = &flow.lanes[li];
        let tx = text(
            t,
            name,
            t.small,
            600,
            ctx.readable(c),
            b.x + 6.0,
            b.y + 4.0 + t.font.baseline_offset(t.small),
            Anchor::Start,
            Role::Label,
        );
        if tx.bounds.right() <= b.right() - 2.0 {
            items.push(Item::Text(tx));
        } else {
            ctx.drop_label(name, DropReason::OutOfBounds, "lane", Some(tx.bounds));
        }
    }
    // Group boxes.
    for (gi, b) in laid.groups.iter().enumerate() {
        if b.w <= 0.0 {
            continue;
        }
        let g = &flow.groups[gi];
        let slot = ctx.series.iter().position(|x| x.key == g.label).unwrap_or(0);
        let c = tone_color(ctx, g.tone).unwrap_or_else(|| ctx.series_color(slot));
        let b = shift(*b, left, top);
        items.push(Item::Rect {
            rect: b,
            fill: Fill::solid(Style::rgba(mix(s.surface, c, 0.025))),
            stroke: Some(Stroke::solid(Style::rgba(mix(s.surface, c, 0.55)), 1.0)),
            radius: [radius.min(6.0); 4],
            role: Role::Band,
            series: Some(slot),
            obs: None,
        });
    }
    // Lines, then arrowheads.
    let edge_obs_base = flow.nodes.len();
    let mut tips: Vec<[f64; 2]> = Vec::new();
    // A failure path first: where it shares a decision's stem, the plain line shows on top.
    let mut paint_order: Vec<usize> = (0..flow.edges.len()).collect();
    paint_order.sort_by_key(|&ei| (flow.edges[ei].tone != Tone::Alert, ei));
    for ei in paint_order {
        let e = &flow.edges[ei];
        let route = &laid.engine.edges[ei];
        if route.points.len() < 2 {
            continue;
        }
        let pts: Vec<[f64; 2]> = route.points.iter().map(|p| page(*p)).collect();
        let c = tone_color(ctx, e.tone)
            .filter(|_| e.tone == Tone::Alert)
            .unwrap_or_else(|| mix(s.surface, s.ink, 0.62));
        let mut stroke = Stroke::solid(Style::rgba(c), if e.tone == Tone::Alert { 1.5 } else { 1.25 });
        if e.dashed || (route.back && !flow.written) {
            stroke.dash = vec![4.0, 3.0];
        }
        // The line stops at the arrowhead's base.
        let mut line = pts.clone();
        let n = line.len();
        let (tip, prev) = (line[n - 1], line[n - 2]);
        let len = (tip[0] - prev[0]).hypot(tip[1] - prev[1]);
        if len > ARROW {
            line[n - 1] = [
                tip[0] - (tip[0] - prev[0]) / len * (ARROW - 1.0),
                tip[1] - (tip[1] - prev[1]) / len * (ARROW - 1.0),
            ];
        }
        items.push(Item::Line {
            path: rounded(&line, 5.0),
            stroke,
            role: Role::Mark,
            series: None,
            obs: Some(edge_obs_base + ei),
        });
        if !tips
            .iter()
            .any(|q| (q[0] - tip[0]).abs() < 0.5 && (q[1] - tip[1]).abs() < 0.5)
        {
            tips.push(tip);
            // The head is no outline of the line: a pointer finds the line by its run.
            items.push(Item::Area {
                path: arrowhead(tip, prev),
                fill: Fill::solid(Style::rgba(c)),
                role: Role::Mark,
                series: None,
                obs: None,
            });
        }
    }
    // Cards.
    let mut node_obs: Vec<Observation> = Vec::new();
    for (i, node) in flow.nodes.iter().enumerate() {
        let r = node_rect(i);
        let card = &laid.cards[i];
        let pt = card_paint(ctx, flow, node);
        let shape = outline(node.kind, r, radius);
        items.push(Item::Area {
            path: shape.clone(),
            fill: Fill::solid(Style::rgba(pt.fill)),
            role: Role::Mark,
            series: None,
            obs: Some(i),
        });
        items.push(Item::Line {
            path: shape,
            stroke: Stroke::solid(Style::rgba(pt.line), 1.2),
            role: Role::Mark,
            series: None,
            obs: Some(i),
        });
        match node.kind {
            Kind::Object => {
                let mut fold = Path::new();
                fold.move_to([r.right() - FOLD, r.y]);
                fold.line_to([r.right() - FOLD, r.y + FOLD]);
                fold.line_to([r.right(), r.y + FOLD]);
                // A detail of the card's outline: the card's own outline is what a hover traces.
                items.push(Item::Line {
                    path: fold,
                    stroke: Stroke::solid(Style::rgba(pt.line), 1.0),
                    role: Role::Mark,
                    series: None,
                    obs: None,
                });
            }
            Kind::Store => {
                let k = 0.552;
                let (rx, ry) = (r.w / 2.0, CAP);
                let cx = r.x + rx;
                let mut lip = Path::new();
                lip.move_to([r.x, r.y + ry]);
                lip.curve_to(
                    [r.x, r.y + ry + ry * k],
                    [cx - rx * k, r.y + 2.0 * ry],
                    [cx, r.y + 2.0 * ry],
                );
                lip.curve_to(
                    [cx + rx * k, r.y + 2.0 * ry],
                    [r.right(), r.y + ry + ry * k],
                    [r.right(), r.y + ry],
                );
                items.push(Item::Line {
                    path: lip,
                    stroke: Stroke::solid(Style::rgba(pt.line), 1.0),
                    role: Role::Mark,
                    series: None,
                    obs: None,
                });
            }
            _ => {}
        }
        // Text: left-set in a card, centred in a diamond or a pill.
        let (lt, lb) = (t.lh(t.title), t.lh(t.body));
        let centred = matches!(node.kind, Kind::Decision | Kind::Start | Kind::End);
        let block_h = card.title.len() as f64 * lt
            + if card.lines.is_empty() {
                0.0
            } else {
                LINES_GAP + card.lines.len() as f64 * lb
            };
        let mut y = if centred {
            r.y + (r.h - block_h) / 2.0
        } else {
            r.y + PAD_Y + if node.kind == Kind::Store { 2.0 * CAP } else { 0.0 }
        };
        let (x, anchor) = if centred {
            (r.x + r.w / 2.0, Anchor::Middle)
        } else {
            (r.x + t.pad_x, Anchor::Start)
        };
        for line in &card.title {
            labels.push(Item::Text(text(
                t,
                line,
                t.title,
                600,
                pt.ink,
                x,
                y + t.font.baseline_offset(t.title),
                anchor,
                Role::Label,
            )));
            y += lt;
        }
        if !card.lines.is_empty() {
            y += LINES_GAP;
        }
        for line in &card.lines {
            labels.push(Item::Text(text(
                t,
                line,
                t.body,
                400,
                pt.soft,
                x,
                y + t.font.baseline_offset(t.body),
                anchor,
                Role::Label,
            )));
            y += lb;
        }
        // The readout: what the card is, where it sits and what joins it.
        let mut lines = Vec::new();
        if node.kind != Kind::Step {
            lines.push(rl("Kind", node.kind.noun().to_owned(), None, false));
        }
        if let Some(g) = node.group {
            let slot = ctx.series.iter().position(|x| x.key == flow.groups[g].label);
            lines.push(rl("Group", flow.groups[g].label.clone(), slot, slot.is_some()));
        }
        if let Some(l) = node.lane {
            let slot = ctx.series.iter().position(|x| x.key == flow.lanes[l]);
            lines.push(rl("Lane", flow.lanes[l].clone(), slot, slot.is_some()));
        }
        if !node.lines.is_empty() {
            lines.push(rl("Detail", node.lines.join("; "), None, false));
        }
        let ins: Vec<String> = flow
            .edges
            .iter()
            .filter(|e| e.to == i)
            .map(|e| flow.nodes[e.from].title.clone())
            .collect();
        let outs: Vec<String> = flow
            .edges
            .iter()
            .filter(|e| e.from == i)
            .map(|e| match &e.label {
                Some(l) => format!("{} ({l})", flow.nodes[e.to].title),
                None => flow.nodes[e.to].title.clone(),
            })
            .collect();
        if !ins.is_empty() {
            lines.push(rl("Comes from", ins.join(", "), None, true));
        }
        if !outs.is_empty() {
            lines.push(rl("Leads to", outs.join(", "), None, true));
        }
        node_obs.push(Observation {
            row: node.row,
            dots: vec![],
            series: None,
            at: r.center(),
            wedge: None,
            area: Some(r),
            radius: 0.0,
            x: r.center()[0],
            readout: Readout {
                heading: node.title.clone(),
                lines,
            }
            .into(),
        });
    }
    observations.extend(node_obs);
    // Edge labels on a patch of the surface, set into or beside their line.
    for (ei, e) in flow.edges.iter().enumerate() {
        let route = &laid.engine.edges[ei];
        let pts: Vec<[f64; 2]> = route.points.iter().map(|p| page(*p)).collect();
        if let Some(text_s) = &e.label {
            match laid.labels[ei] {
                Some(b) => {
                    let b = shift(b, left, top);
                    let c = if e.tone == Tone::Alert {
                        ctx.readable(s.alert)
                    } else {
                        s.ink_soft
                    };
                    labels.push(Item::Rect {
                        rect: b,
                        fill: Fill::solid(Style::rgba(s.surface)),
                        stroke: None,
                        radius: [2.0; 4],
                        role: Role::Annotation,
                        series: None,
                        obs: None,
                    });
                    labels.push(Item::Text(text(
                        t,
                        text_s,
                        t.small,
                        400,
                        c,
                        b.x + b.w / 2.0,
                        b.y + t.font.baseline_offset(t.small),
                        Anchor::Middle,
                        Role::Label,
                    )));
                }
                None => ctx.drop_label(text_s, DropReason::Collision, "connection", None),
            }
        }
        // The pointer target: the longest run of the line.
        let (a, b) = pts
            .windows(2)
            .map(|w| (w[0], w[1]))
            .max_by(|p, q| seg_len(*p).total_cmp(&seg_len(*q)))
            .unwrap_or(([0.0; 2], [0.0; 2]));
        let at = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        let area = Rect::new(a[0].min(b[0]), a[1].min(b[1]), (a[0] - b[0]).abs(), (a[1] - b[1]).abs()).inflate(3.0);
        let mut lines = Vec::new();
        if let Some(l) = &e.label {
            lines.push(rl("Label", l.clone(), None, false));
        }
        if route.back {
            lines.push(rl("Direction", "Returns to an earlier step".into(), None, true));
        } else if e.dashed {
            lines.push(rl("Line", "Dashed: context or feedback".into(), None, false));
        }
        observations.push(Observation {
            row: e.row,
            dots: vec![],
            series: None,
            at,
            wedge: None,
            area: Some(area),
            radius: 0.0,
            x: at[0],
            readout: Readout {
                heading: format!("{} → {}", flow.nodes[e.from].title, flow.nodes[e.to].title),
                lines,
            }
            .into(),
        });
    }
    // Group names, set into the border.
    for (gi, b) in laid.group_labels.iter().enumerate() {
        let Some(b) = b else {
            if laid.groups.get(gi).is_some_and(|g| g.w > 0.0) {
                ctx.drop_label(&flow.groups[gi].label, DropReason::Collision, "group", None);
            }
            continue;
        };
        let g = &flow.groups[gi];
        let slot = ctx.series.iter().position(|x| x.key == g.label).unwrap_or(0);
        let c = tone_color(ctx, g.tone).unwrap_or_else(|| ctx.series_color(slot));
        let b = shift(*b, left, top);
        labels.push(Item::Rect {
            rect: b,
            fill: Fill::solid(Style::rgba(s.surface)),
            stroke: None,
            radius: [0.0; 4],
            role: Role::Annotation,
            series: None,
            obs: None,
        });
        labels.push(Item::Text(text(
            t,
            &g.label,
            t.small,
            600,
            ctx.readable(c),
            b.x + 4.0,
            b.y + t.font.baseline_offset(t.small),
            Anchor::Start,
            Role::Label,
        )));
    }
    items.extend(labels);
    // Edges' readouts follow every card's, in drawing order of the cards: rank, then position.
    let mut order: Vec<usize> = (0..flow.nodes.len()).collect();
    order.sort_by(|&a, &b| {
        let (ra, rb) = (laid.engine.ranks[a], laid.engine.ranks[b]);
        let (pa, pb) = (node_rect(a), node_rect(b));
        ra.cmp(&rb)
            .then(if laid.across {
                pa.y.total_cmp(&pb.y)
            } else {
                pa.x.total_cmp(&pb.x)
            })
            .then(a.cmp(&b))
    });
    let mut remap = vec![0usize; observations.len()];
    let mut sorted: Vec<Observation> = Vec::with_capacity(observations.len());
    for (new, &old) in order.iter().enumerate() {
        remap[old] = new;
        sorted.push(observations[old].clone());
    }
    for (k, o) in observations.iter().enumerate().skip(flow.nodes.len()) {
        remap[k] = sorted.len();
        sorted.push(o.clone());
    }
    for item in &mut items {
        match item {
            Item::Rect { obs: Some(o), .. }
            | Item::Line { obs: Some(o), .. }
            | Item::Area { obs: Some(o), .. }
            | Item::Point { obs: Some(o), .. } => *o = remap[*o],
            _ => {}
        }
    }
    Panel {
        facet: None,
        frame: Rect::default(),
        plot: Rect::default(),
        items,
        observations: sorted,
        targeting: Targeting::Contain,
        x: None,
        failure: None,
    }
}

fn rl(name: &str, value: String, series: Option<usize>, derived: bool) -> ReadoutLine {
    ReadoutLine {
        name: name.into(),
        value,
        series,
        names_series: series.is_some(),
        derived,
    }
}

fn seg_len((a, b): ([f64; 2], [f64; 2])) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_read_with_arrows_dashes_and_labels() {
        assert_eq!(parse_edge("a -> b"), Some(("a".into(), "b".into(), None, false)));
        assert_eq!(
            parse_edge("Valid? -> Store: yes"),
            Some(("Valid?".into(), "Store".into(), Some("yes".into()), false))
        );
        assert_eq!(
            parse_edge("wait ..> fresh: ask again"),
            Some(("wait".into(), "fresh".into(), Some("ask again".into()), true))
        );
        assert_eq!(parse_edge("nothing"), None);
        assert_eq!(parse_edge(" -> b"), None);
    }

    #[test]
    fn a_title_gives_its_id() {
        assert_eq!(slug("Score every open dock"), "score-every-open-dock");
        assert_eq!(slug("publish = true?"), "publish-true");
    }

    #[test]
    fn table_rows_list_cards_then_edges() {
        let block = json!({
            "nodes": [{"title": "Read"}, {"title": "Ok?", "kind": "decision"}],
            "edges": ["Read -> ok: checked"],
            "groups": [{"label": "Job", "nodes": ["read"]}]
        });
        let rows = table_rows(&block).unwrap();
        assert_eq!(rows[0], json!({"step": "Read", "kind": "step", "group": "Job"}));
        assert_eq!(rows[1], json!({"step": "Ok?", "kind": "decision"}));
        assert_eq!(rows[2], json!({"from": "Read", "to": "Ok?", "label": "checked"}));
    }
}
