//! The layered layout behind flow charts, in abstract coordinates: `u` runs along a rank (across
//! the flow) and `v` along the flow, so one engine draws top-down and left-to-right flows. The
//! caller measures every card and label in those axes and turns the result back into page
//! coordinates.
//!
//! The steps follow the Sugiyama framework as Graphviz's dot applies it, with the choices that
//! make a flow chart read rather than merely place:
//!
//! - A depth-first walk in source order finds the edges that close a cycle; they loop back round
//!   the outside of the drawing instead of inverting the flow.
//! - Ranks are the longest path from a start; a card that only feeds others sits just above the
//!   first card it feeds, so its edge stays short.
//! - A long edge gets one placeholder per rank it crosses, so it runs in its own channel between
//!   the cards; a labelled long edge reserves its label's room on its middle placeholder.
//! - Crossings fall by barycentre sweeps and adjacent swaps; a group's cards stay together in
//!   every rank, and placeholders fill the ranks a group spans without a card, so nothing foreign
//!   lands inside its box.
//! - Positions along a rank minimise the squared distance to each card's neighbours under the
//!   rank's order and spacing, solved exactly by isotonic regression; chains of placeholders
//!   weigh more, so long edges run straight, and walls keep other cards out of group boxes.
//! - Every edge meets a card at its own point; two cards whose sides overlap are joined by a
//!   straight line; each bend gets its own track in the gap, ordered to avoid crossings; a
//!   decision leaves by its side corners.
//! - An edge label sits on free paper in its gap, set into its line or beside it; when no place
//!   is free the gap grows. No label is ever drawn over a card, another label or another line.
//!
//! Nothing is random and no map is iterated: the same input gives the same geometry.

use crate::geometry::Rect;

/// How a card meets its edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ports {
    /// Edges spread along the side facing the flow.
    Side,
    /// A diamond: edges meet its corners, and a second or third exit leaves by a side corner.
    Corners,
}

#[derive(Debug, Clone)]
pub(super) struct NodeIn {
    /// Extent along a rank.
    pub breadth: f64,
    /// Extent along the flow.
    pub depth: f64,
    pub group: Option<usize>,
    pub lane: Option<usize>,
    pub ports: Ports,
}

#[derive(Debug, Clone)]
pub(super) struct EdgeIn {
    pub from: usize,
    pub to: usize,
    /// The label's extent: `[breadth, depth]`.
    pub label: Option<[f64; 2]>,
}

#[derive(Debug, Clone)]
pub(super) struct GroupIn {
    /// The label's extent: `[breadth, depth]`.
    pub label: [f64; 2],
}

#[derive(Debug, Clone)]
pub(super) struct Params {
    /// Space between two cards of a rank.
    pub node_gap: f64,
    /// Least space between two ranks.
    pub rank_gap: f64,
    /// Space between two parallel lines.
    pub track_gap: f64,
    /// Space between a group's border and its cards.
    pub group_pad: f64,
    /// Space between two lanes, and between a lane's edge and its cards.
    pub lane_pad: f64,
    /// Length of an arrowhead.
    pub arrow: f64,
    /// Space round a label.
    pub label_pad: f64,
    /// Room each lane keeps at its start for its name: `[breadth, depth]`.
    pub lane_label: [f64; 2],
}

pub(super) struct Input {
    pub nodes: Vec<NodeIn>,
    pub edges: Vec<EdgeIn>,
    pub groups: Vec<GroupIn>,
    pub lanes: usize,
    pub params: Params,
}

/// Which side of a card an edge meets: in abstract axes `Start` faces lower `v` (up the flow).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Before,
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub(super) struct Route {
    /// The line from the source's side to the target's side, corners only.
    pub points: Vec<[f64; 2]>,
    /// Closes a cycle: it runs back up the flow.
    pub back: bool,
    pub label: Option<Rect>,
    /// The side of the target the arrow meets.
    pub end: Side,
}

pub(super) struct Laid {
    pub nodes: Vec<Rect>,
    pub ranks: Vec<usize>,
    pub edges: Vec<Route>,
    pub groups: Vec<Rect>,
    pub group_labels: Vec<Rect>,
    pub lanes: Vec<Rect>,
    pub extent: [f64; 2],
    pub crossings: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Real(usize),
    /// A placeholder of a long edge.
    Dummy {
        edge: usize,
    },
    /// Keeps a rank's place inside a group or lane that has no card there.
    Fill,
}

#[derive(Debug, Clone)]
struct V {
    kind: Kind,
    breadth: f64,
    rank: usize,
    group: Option<usize>,
    lane: Option<usize>,
    u: f64,
}

impl V {
    fn is_dummy(&self) -> bool {
        matches!(self.kind, Kind::Dummy { .. })
    }
}

/// A forward edge as the placeholders it passes, and the rank of each step.
#[derive(Debug, Clone)]
struct Chain {
    edge: usize,
    vs: Vec<usize>,
}

const EPS: f64 = 0.5;

pub(super) fn layout(input: &Input) -> Laid {
    let n = input.nodes.len();
    let back = cycles(n, &input.edges);
    let ranks = rank(n, &input.edges, &back);
    let mut g = build(input, &ranks, &back);
    order(&mut g);
    place(&mut g, input);
    let mut extra: Vec<f64> = vec![0.0; g.ranks + 1];
    // A label that finds no free place grows its gap, and the drawing is routed again.
    for _ in 0..6 {
        let laid = draw(&g, input, &back, &extra);
        if laid.0.is_empty() {
            return laid.1;
        }
        let mut grown = false;
        for gap in laid.0 {
            if gap < extra.len() && extra[gap] < 200.0 {
                extra[gap] += input
                    .edges
                    .iter()
                    .filter_map(|e| e.label.map(|l| l[1]))
                    .fold(0.0, f64::max)
                    + input.params.label_pad * 2.0;
                grown = true;
            }
        }
        if !grown {
            return laid.1;
        }
    }
    draw(&g, input, &back, &extra).1
}

/// The edges a depth-first walk in source order meets while their target is still open.
fn cycles(n: usize, edges: &[EdgeIn]) -> Vec<bool> {
    let mut out: Vec<Vec<usize>> = vec![vec![]; n];
    let mut indegree = vec![0usize; n];
    for (k, e) in edges.iter().enumerate() {
        out[e.from].push(k);
        indegree[e.to] += 1;
    }
    let mut back = vec![false; edges.len()];
    // 0 unseen, 1 open, 2 closed. Starts first, so a loop returns to where the flow begins.
    let mut state = vec![0u8; n];
    let roots: Vec<usize> = (0..n).filter(|&k| indegree[k] == 0).chain(0..n).collect();
    for root in roots {
        if state[root] != 0 {
            continue;
        }
        state[root] = 1;
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some((u, i)) = stack.pop() {
            if i >= out[u].len() {
                state[u] = 2;
                continue;
            }
            stack.push((u, i + 1));
            let k = out[u][i];
            let v = edges[k].to;
            match state[v] {
                1 => back[k] = true,
                0 => {
                    state[v] = 1;
                    stack.push((v, 0));
                }
                _ => {}
            }
        }
    }
    back
}

/// Longest-path ranks over the forward edges; a card with no inputs then moves down to just
/// above the first card it feeds.
fn rank(n: usize, edges: &[EdgeIn], back: &[bool]) -> Vec<usize> {
    let mut ranks = vec![0usize; n];
    let mut indegree = vec![0usize; n];
    let forward: Vec<&EdgeIn> = edges.iter().zip(back).filter(|(_, b)| !**b).map(|(e, _)| e).collect();
    for e in &forward {
        indegree[e.to] += 1;
    }
    let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|&k| indegree[k] == 0).collect();
    let mut left = indegree.clone();
    while let Some(u) = queue.pop_front() {
        for e in forward.iter().filter(|e| e.from == u) {
            ranks[e.to] = ranks[e.to].max(ranks[u] + 1);
            left[e.to] -= 1;
            if left[e.to] == 0 {
                queue.push_back(e.to);
            }
        }
    }
    for _ in 0..n.min(8) {
        let mut moved = false;
        for k in 0..n {
            if indegree[k] != 0 {
                continue;
            }
            let lowest = forward.iter().filter(|e| e.from == k).map(|e| ranks[e.to]).min();
            if let Some(r) = lowest
                && r >= 1
                && r - 1 > ranks[k]
            {
                ranks[k] = r - 1;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    ranks
}

struct G {
    vs: Vec<V>,
    ranks: usize,
    /// Vertices of each rank, in order.
    order: Vec<Vec<usize>>,
    chains: Vec<Chain>,
    /// Each vertex's neighbours in the rank above and the rank below, with their weights.
    up: Vec<Vec<(usize, f64)>>,
    down: Vec<Vec<(usize, f64)>>,
    /// The first and last rank of each group.
    spans: Vec<Option<(usize, usize)>>,
}

fn build(input: &Input, ranks: &[usize], back: &[bool]) -> G {
    let mut vs: Vec<V> = input
        .nodes
        .iter()
        .enumerate()
        .map(|(i, nd)| V {
            kind: Kind::Real(i),
            breadth: nd.breadth,
            rank: ranks[i],
            group: nd.group,
            lane: nd.lane,
            u: 0.0,
        })
        .collect();
    let count = ranks.iter().copied().max().map_or(1, |m| m + 1);
    let p = &input.params;
    let mut chains = Vec::new();
    let mut links = Vec::new();
    for (k, e) in input.edges.iter().enumerate() {
        if back[k] || e.from == e.to {
            continue;
        }
        let (a, b) = (ranks[e.from], ranks[e.to]);
        let mut chain = vec![e.from];
        let span = b - a;
        let same_group = input.nodes[e.from]
            .group
            .filter(|g| input.nodes[e.to].group == Some(*g));
        for (step, r) in (a + 1..b).enumerate() {
            // The middle placeholder of a labelled edge holds its label.
            let breadth = match e.label {
                Some(l) if step == (span - 1) / 2 => l[0] + 2.0 * p.label_pad,
                _ => 0.0,
            };
            let lane = if step * 2 < span {
                input.nodes[e.from].lane
            } else {
                input.nodes[e.to].lane
            };
            vs.push(V {
                kind: Kind::Dummy { edge: k },
                breadth,
                rank: r,
                group: same_group,
                lane,
                u: 0.0,
            });
            chain.push(vs.len() - 1);
        }
        chain.push(e.to);
        for w in chain.windows(2) {
            let weight = match (vs[w[0]].is_dummy(), vs[w[1]].is_dummy()) {
                (true, true) => 8.0,
                (false, false) => 1.0,
                _ => 2.0,
            };
            links.push((w[0], w[1], weight));
        }
        chains.push(Chain { edge: k, vs: chain });
    }
    let mut spans: Vec<Option<(usize, usize)>> = vec![None; input.groups.len()];
    for v in &vs {
        if let Some(g) = v.group {
            let s = spans[g].get_or_insert((v.rank, v.rank));
            s.0 = s.0.min(v.rank);
            s.1 = s.1.max(v.rank);
        }
    }
    // A group keeps its place in every rank it spans, a lane in every rank.
    for (g, span) in spans.iter().enumerate() {
        let Some((a, b)) = *span else { continue };
        let lane = vs.iter().find(|v| v.group == Some(g)).and_then(|v| v.lane);
        for r in a..=b {
            if !vs.iter().any(|v| v.rank == r && v.group == Some(g)) {
                vs.push(V {
                    kind: Kind::Fill,
                    breadth: 0.0,
                    rank: r,
                    group: Some(g),
                    lane,
                    u: 0.0,
                });
            }
        }
    }
    for lane in 0..input.lanes {
        for r in 0..count {
            if !vs.iter().any(|v| v.rank == r && v.lane == Some(lane)) {
                vs.push(V {
                    kind: Kind::Fill,
                    breadth: 0.0,
                    rank: r,
                    group: None,
                    lane: Some(lane),
                    u: 0.0,
                });
            }
        }
    }
    // First order: a depth-first walk from the starts in source order, as dot begins.
    let mut order: Vec<Vec<usize>> = vec![vec![]; count];
    let mut seen = vec![false; vs.len()];
    let mut below: Vec<Vec<usize>> = vec![vec![]; vs.len()];
    let mut has_above = vec![false; vs.len()];
    for &(a, b, _) in &links {
        below[a].push(b);
        has_above[b] = true;
    }
    let starts: Vec<usize> = (0..vs.len()).filter(|&i| !has_above[i]).chain(0..vs.len()).collect();
    for s in starts {
        if seen[s] {
            continue;
        }
        let mut stack = vec![s];
        while let Some(v) = stack.pop() {
            if seen[v] {
                continue;
            }
            seen[v] = true;
            order[vs[v].rank].push(v);
            for &w in below[v].iter().rev() {
                if !seen[w] {
                    stack.push(w);
                }
            }
        }
    }
    let mut up: Vec<Vec<(usize, f64)>> = vec![vec![]; vs.len()];
    let mut down: Vec<Vec<(usize, f64)>> = vec![vec![]; vs.len()];
    for &(a, b, w) in &links {
        down[a].push((b, w));
        up[b].push((a, w));
    }
    G {
        vs,
        ranks: count,
        order,
        chains,
        up,
        down,
        spans,
    }
}

/// Crossings between rank `r` and the next.
fn crossings_at(g: &G, pos: &[usize], r: usize) -> usize {
    let segs: Vec<(usize, usize)> = g.order[r]
        .iter()
        .flat_map(|&a| g.down[a].iter().map(move |&(b, _)| (pos[a], pos[b])))
        .collect();
    let mut c = 0;
    for i in 0..segs.len() {
        for j in i + 1..segs.len() {
            let (a, b) = (segs[i], segs[j]);
            if (a.0 < b.0 && a.1 > b.1) || (a.0 > b.0 && a.1 < b.1) {
                c += 1;
            }
        }
    }
    c
}

fn positions(g: &G) -> Vec<usize> {
    let mut pos = vec![0usize; g.vs.len()];
    for rank in &g.order {
        for (i, &v) in rank.iter().enumerate() {
            pos[v] = i;
        }
    }
    pos
}

fn total_crossings(g: &G) -> usize {
    let pos = positions(g);
    (0..g.ranks.saturating_sub(1)).map(|r| crossings_at(g, &pos, r)).sum()
}

/// The block a vertex moves with: its group, else itself.
fn unit(v: &V, i: usize) -> (usize, usize) {
    match v.group {
        Some(gr) => (0, gr),
        None => (1, i),
    }
}

fn order(g: &mut G) {
    // The first order keeps every lane and every group in one piece too: it is the answer when
    // no sweep crosses fewer lines, and placement keeps other cards out of a lane's or a group's
    // box, which nothing can do for cards that stand inside it.
    for r in 0..g.ranks {
        let own: Vec<(usize, f64)> = g.order[r].iter().enumerate().map(|(i, &v)| (v, i as f64)).collect();
        sort_rank(g, r, &own);
    }
    let mut best = g.order.clone();
    let mut best_c = total_crossings(g);
    for pass in 0..16 {
        let down = pass % 2 == 0;
        let pos = positions(g);
        let ranks: Vec<usize> = if down {
            (1..g.ranks).collect()
        } else {
            (0..g.ranks.saturating_sub(1)).rev().collect()
        };
        let mut pos = pos;
        for r in ranks {
            let bary: Vec<(usize, f64)> = g.order[r]
                .iter()
                .map(|&v| {
                    let nbrs = if down { &g.up[v] } else { &g.down[v] };
                    let (sum, total) = nbrs
                        .iter()
                        .fold((0.0, 0.0), |(s, t), &(o, w)| (s + pos[o] as f64 * w, t + w));
                    let b = if total == 0.0 { pos[v] as f64 } else { sum / total };
                    (v, b)
                })
                .collect();
            sort_rank(g, r, &bary);
            for (i, &v) in g.order[r].iter().enumerate() {
                pos[v] = i;
            }
        }
        transpose(g);
        let c = total_crossings(g);
        if c < best_c {
            best_c = c;
            best = g.order.clone();
        }
        if best_c == 0 {
            break;
        }
    }
    g.order = best;
}

/// Order a rank by lane, then by the barycentre of its block, then by its own; ties keep order.
fn sort_rank(g: &mut G, r: usize, bary: &[(usize, f64)]) {
    let at = |v: usize| bary.iter().find(|b| b.0 == v).map_or(0.0, |b| b.1);
    let current: Vec<usize> = g.order[r].clone();
    let index = |v: usize| current.iter().position(|&w| w == v).unwrap_or(0);
    let mut block_mean: Vec<((usize, usize), f64)> = Vec::new();
    for &v in &current {
        let key = unit(&g.vs[v], v);
        if block_mean.iter().any(|b| b.0 == key) {
            continue;
        }
        let members: Vec<f64> = current
            .iter()
            .filter(|&&w| unit(&g.vs[w], w) == key)
            .map(|&w| at(w))
            .collect();
        block_mean.push((key, members.iter().sum::<f64>() / members.len() as f64));
    }
    let mean = |key: (usize, usize)| block_mean.iter().find(|b| b.0 == key).map_or(0.0, |b| b.1);
    let mut keyed: Vec<usize> = current.clone();
    keyed.sort_by(|&a, &b| {
        let (va, vb) = (&g.vs[a], &g.vs[b]);
        let (ka, kb) = (unit(va, a), unit(vb, b));
        va.lane
            .unwrap_or(0)
            .cmp(&vb.lane.unwrap_or(0))
            .then(mean(ka).total_cmp(&mean(kb)))
            .then(ka.cmp(&kb))
            .then(at(a).total_cmp(&at(b)))
            .then(index(a).cmp(&index(b)))
    });
    g.order[r] = keyed;
}

/// Swap neighbours of one block while that removes crossings.
fn transpose(g: &mut G) {
    for _ in 0..4 {
        let mut improved = false;
        for r in 0..g.ranks {
            for i in 0..g.order[r].len().saturating_sub(1) {
                let (a, b) = (g.order[r][i], g.order[r][i + 1]);
                let (va, vb) = (&g.vs[a], &g.vs[b]);
                if va.lane != vb.lane || unit(va, a).0 != unit(vb, b).0 || (va.group.is_some() && va.group != vb.group)
                {
                    continue;
                }
                let pos = positions(g);
                let before = around(g, &pos, r);
                g.order[r].swap(i, i + 1);
                let pos = positions(g);
                let after = around(g, &pos, r);
                if after < before {
                    improved = true;
                } else {
                    g.order[r].swap(i, i + 1);
                }
            }
        }
        if !improved {
            break;
        }
    }
}

fn around(g: &G, pos: &[usize], r: usize) -> usize {
    let mut c = 0;
    if r > 0 {
        c += crossings_at(g, pos, r - 1);
    }
    if r + 1 < g.ranks {
        c += crossings_at(g, pos, r);
    }
    c
}

/// Least centre distance between two neighbours of a rank.
fn separation(g: &G, input: &Input, a: usize, b: usize) -> f64 {
    let p = &input.params;
    let (va, vb) = (&g.vs[a], &g.vs[b]);
    let mut s = (va.breadth + vb.breadth) / 2.0;
    s += match (va.kind, vb.kind) {
        (Kind::Fill, _) | (_, Kind::Fill) => 0.0,
        (Kind::Dummy { .. }, Kind::Dummy { .. }) => p.track_gap * 1.5,
        (Kind::Dummy { .. }, _) | (_, Kind::Dummy { .. }) => p.node_gap * 0.6,
        _ => p.node_gap,
    };
    if va.group != vb.group {
        if va.group.is_some() {
            s += p.group_pad;
        }
        if vb.group.is_some() {
            s += p.group_pad;
        }
        if va.group.is_some() && vb.group.is_some() {
            s += p.node_gap * 0.5;
        }
    }
    if va.lane != vb.lane {
        s += 2.0 * p.lane_pad + p.lane_label[0];
    }
    s
}

/// Weighted isotonic regression (pool adjacent violators): the positions nearest `want` that
/// keep each neighbour at least its separation away.
fn project(want: &[f64], weight: &[f64], sep: &[f64]) -> Vec<f64> {
    let n = want.len();
    let mut offset = vec![0.0; n];
    for i in 1..n {
        offset[i] = offset[i - 1] + sep[i - 1];
    }
    // Blocks of (mean, weight, count).
    let mut blocks: Vec<(f64, f64, usize)> = Vec::new();
    for i in 0..n {
        blocks.push((want[i] - offset[i], weight[i], 1));
        while blocks.len() > 1 {
            let l = blocks.len();
            if blocks[l - 2].0 <= blocks[l - 1].0 {
                break;
            }
            let (m2, w2, c2) = blocks.pop().unwrap();
            let (m1, w1, c1) = blocks.pop().unwrap();
            let w = w1 + w2;
            blocks.push(((m1 * w1 + m2 * w2) / w, w, c1 + c2));
        }
    }
    let mut out = Vec::with_capacity(n);
    for (m, _, c) in blocks {
        for _ in 0..c {
            out.push(m);
        }
    }
    for i in 0..n {
        out[i] += offset[i];
    }
    out
}

fn place(g: &mut G, input: &Input) {
    // Start packed from the left.
    for r in 0..g.ranks {
        let mut x = 0.0;
        for i in 0..g.order[r].len() {
            let v = g.order[r][i];
            if i > 0 {
                x += separation(g, input, g.order[r][i - 1], v);
            }
            g.vs[v].u = x;
        }
    }
    let mut walls: Vec<Vec<f64>> = g.order.iter().map(|o| vec![0.0; o.len()]).collect();
    for round in 0..24 {
        let sweeps: Vec<(usize, i32)> = match round % 3 {
            0 => (1..g.ranks).map(|r| (r, -1)).collect(),
            1 => (0..g.ranks.saturating_sub(1)).rev().map(|r| (r, 1)).collect(),
            _ => (0..g.ranks).map(|r| (r, 0)).collect(),
        };
        let boxes = container_boxes(g, input);
        for (r, dir) in sweeps {
            let ord = g.order[r].clone();
            let mut want = Vec::with_capacity(ord.len());
            let mut weight = Vec::with_capacity(ord.len());
            for &v in &ord {
                let (mut sum, mut total) = (0.0, 0.0);
                let above = if dir <= 0 { &g.up[v][..] } else { &[][..] };
                let below = if dir >= 0 { &g.down[v][..] } else { &[][..] };
                let me = &g.vs[v];
                // A lane is a band of its own: nothing pulls a card toward another lane, where
                // the wall between the two would only push the lanes apart again.
                for &(o, w) in above.iter().chain(below) {
                    if g.vs[o].lane != me.lane {
                        continue;
                    }
                    sum += g.vs[o].u * w;
                    total += w;
                }
                let w = if matches!(me.kind, Kind::Fill) {
                    // A fill sits in the middle of its container.
                    let c = me.group.map(|gr| boxes.0[gr]).or_else(|| me.lane.map(|l| boxes.1[l]));
                    if let Some(Some((l, rt))) = c {
                        sum = (l + rt) / 2.0;
                        total = 1.0;
                    }
                    0.2
                } else if me.is_dummy() {
                    2.0
                } else {
                    1.0
                };
                want.push(if total > 0.0 {
                    (sum / total) * 0.85 + me.u * 0.15
                } else {
                    me.u
                });
                weight.push(w);
            }
            let sep: Vec<f64> = (0..ord.len().saturating_sub(1))
                .map(|i| separation(g, input, ord[i], ord[i + 1]) + walls[r][i])
                .collect();
            let placed = project(&want, &weight, &sep);
            for (i, &v) in ord.iter().enumerate() {
                g.vs[v].u = placed[i];
            }
        }
        // Walls: a card next to a group or lane keeps clear of all of its box, not only of the
        // part in its own rank.
        let boxes = container_boxes(g, input);
        for (r, row) in walls.iter_mut().enumerate() {
            let pairs = row.len().saturating_sub(1);
            for (i, slot) in row.iter_mut().enumerate().take(pairs) {
                let (a, b) = (g.order[r][i], g.order[r][i + 1]);
                let (va, vb) = (&g.vs[a], &g.vs[b]);
                let mut wall: f64 = 0.0;
                if va.group != vb.group {
                    if let Some(Some((_, right))) = va.group.map(|gr| boxes.0[gr]) {
                        wall += (right - (va.u + va.breadth / 2.0)).max(0.0);
                    }
                    if let Some(Some((left, _))) = vb.group.map(|gr| boxes.0[gr]) {
                        wall += ((vb.u - vb.breadth / 2.0) - left).max(0.0);
                    }
                }
                if va.lane != vb.lane {
                    if let Some(Some((_, right))) = va.lane.map(|l| boxes.1[l]) {
                        wall += (right - (va.u + va.breadth / 2.0)).max(0.0);
                    }
                    if let Some(Some((left, _))) = vb.lane.map(|l| boxes.1[l]) {
                        wall += ((vb.u - vb.breadth / 2.0) - left).max(0.0);
                    }
                }
                *slot = wall;
            }
        }
    }
    // Lanes in order, each clear of the one before: a lane moves whole, so its own order and
    // spacing stay.
    let p = &input.params;
    for lane in 1..input.lanes {
        let boxes = container_boxes(g, input);
        let (Some(prev), Some(this)) = ((0..lane).rev().find_map(|l| boxes.1[l]), boxes.1[lane]) else {
            continue;
        };
        let need = prev.1 + 2.0 * p.lane_pad + p.lane_label[0] + p.node_gap - this.0;
        if need > 0.0 {
            for v in &mut g.vs {
                if v.lane.is_some_and(|l| l >= lane) {
                    v.u += need;
                }
            }
        }
    }
    let min = g.vs.iter().map(|v| v.u - v.breadth / 2.0).fold(f64::INFINITY, f64::min);
    if min.is_finite() {
        for v in &mut g.vs {
            v.u -= min;
        }
    }
}

type Span = Option<(f64, f64)>;

/// The extent along a rank of each group's and each lane's cards.
fn container_boxes(g: &G, input: &Input) -> (Vec<Span>, Vec<Span>) {
    let mut groups: Vec<Span> = vec![None; input.groups.len()];
    let mut lanes: Vec<Span> = vec![None; input.lanes];
    for v in &g.vs {
        if matches!(v.kind, Kind::Fill) {
            continue;
        }
        let (l, r) = (v.u - v.breadth / 2.0, v.u + v.breadth / 2.0);
        if let Some(gr) = v.group {
            let s = groups[gr].get_or_insert((l, r));
            *s = (s.0.min(l), s.1.max(r));
        }
        if let Some(ln) = v.lane {
            let s = lanes[ln].get_or_insert((l, r));
            *s = (s.0.min(l), s.1.max(r));
        }
    }
    (groups, lanes)
}

/// One hop of an edge between two consecutive ranks.
#[derive(Debug, Clone)]
struct Hop {
    edge: usize,
    gap: usize,
    from: f64,
    to: f64,
    track: usize,
}

/// Where the ranks and the gaps between them sit along the flow.
struct Rows {
    top: Vec<f64>,
    depth: Vec<f64>,
    /// For each gap below a rank: where its label rows, tracks and group borders sit.
    label_v: Vec<f64>,
    track_v: Vec<f64>,
    group_end_v: Vec<f64>,
    group_start_v: Vec<f64>,
}

/// Route, label and measure the drawing. Returns the gaps whose labels found no place.
#[allow(clippy::too_many_lines)]
fn draw(g: &G, input: &Input, back: &[bool], extra: &[f64]) -> (Vec<usize>, Laid) {
    let p = &input.params;
    let n = input.nodes.len();
    // Ports: where each edge leaves its source and meets its target.
    let mut exit_u: Vec<Option<f64>> = vec![None; input.edges.len()];
    let mut entry_u: Vec<Option<f64>> = vec![None; input.edges.len()];
    let mut side_exit: Vec<Option<Side>> = vec![None; input.edges.len()];
    let first_hop = |c: &Chain| c.vs[1];
    let last_hop = |c: &Chain| c.vs[c.vs.len() - 2];
    let uv = |v: usize| g.vs[v].u;
    let left = |i: usize| g.vs[i].u - g.vs[i].breadth / 2.0;
    let right = |i: usize| g.vs[i].u + g.vs[i].breadth / 2.0;
    // Decisions: one exit straight on, the others by the side corners when the rank is clear.
    for i in 0..n {
        if input.nodes[i].ports != Ports::Corners {
            continue;
        }
        let mut outs: Vec<&Chain> = g.chains.iter().filter(|c| input.edges[c.edge].from == i).collect();
        outs.sort_by(|a, b| uv(first_hop(a)).total_cmp(&uv(first_hop(b))).then(a.edge.cmp(&b.edge)));
        if outs.len() >= 2 {
            let me = g.vs[i].u;
            // The exit most nearly straight on keeps the bottom corner.
            let straight = outs
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    (uv(first_hop(a.1)) - me)
                        .abs()
                        .total_cmp(&(uv(first_hop(b.1)) - me).abs())
                })
                .map_or(0, |x| x.0);
            for (k, c) in outs.iter().enumerate() {
                if k == straight {
                    continue;
                }
                let target = uv(first_hop(c));
                let side = if target < me { Side::Left } else { Side::Right };
                // A side used by an earlier exit, or a card between the corner and the turn,
                // keeps the edge on the bottom corner.
                if side_exit
                    .iter()
                    .zip(&input.edges)
                    .any(|(s, e)| e.from == i && *s == Some(side))
                {
                    continue;
                }
                let r = g.vs[i].rank;
                let (lo, hi) = if side == Side::Left {
                    (target - 1.0, left(i))
                } else {
                    (right(i), target + 1.0)
                };
                let blocked = g.order[r].iter().any(|&w| {
                    w != i && !matches!(g.vs[w].kind, Kind::Fill) && right(w) + 2.0 > lo && left(w) - 2.0 < hi
                });
                if !blocked && (side == Side::Left) == (target < left(i))
                    || (!blocked && side == Side::Right && target > right(i))
                {
                    side_exit[c.edge] = Some(side);
                    exit_u[c.edge] = Some(target);
                }
            }
        }
        for c in &outs {
            if side_exit[c.edge].is_none() {
                exit_u[c.edge] = Some(g.vs[i].u);
            }
        }
        for c in g.chains.iter().filter(|c| input.edges[c.edge].to == i) {
            entry_u[c.edge] = Some(g.vs[i].u);
        }
    }
    // Cards: edges spread along the side, in the order of what they lead to.
    let spread = |i: usize, ends: &mut Vec<(usize, f64)>| -> Vec<(usize, f64)> {
        let margin = (g.vs[i].breadth / 4.0).min(12.0);
        let (lo, hi) = (left(i) + margin, right(i) - margin);
        ends.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        let k = ends.len();
        if k == 1 {
            return vec![(ends[0].0, ends[0].1.clamp(lo, hi.max(lo)))];
        }
        let step = ((hi - lo) / (k as f64 - 1.0)).min(p.track_gap * 2.5).max(0.0);
        let mean = ends.iter().map(|e| e.1).sum::<f64>() / k as f64;
        let half = step * (k as f64 - 1.0) / 2.0;
        let centre = mean.clamp(lo + half, (hi - half).max(lo + half));
        ends.iter()
            .enumerate()
            .map(|(j, e)| (e.0, centre - half + j as f64 * step))
            .collect()
    };
    for i in 0..n {
        if input.nodes[i].ports == Ports::Corners {
            continue;
        }
        let mut outs: Vec<(usize, f64)> = g
            .chains
            .iter()
            .filter(|c| input.edges[c.edge].from == i)
            .map(|c| (c.edge, uv(first_hop(c))))
            .collect();
        if !outs.is_empty() {
            for (e, u) in spread(i, &mut outs) {
                exit_u[e] = Some(u);
            }
        }
        // An edge that left a decision by its side corner drops straight in under its turn.
        let mut ins: Vec<(usize, f64)> = g
            .chains
            .iter()
            .filter(|c| input.edges[c.edge].to == i)
            .map(|c| {
                let reference = match (side_exit[c.edge], c.vs.len()) {
                    (Some(_), 2) => exit_u[c.edge].unwrap_or(uv(last_hop(c))),
                    _ => uv(last_hop(c)),
                };
                (c.edge, reference)
            })
            .collect();
        if !ins.is_empty() {
            for (e, u) in spread(i, &mut ins) {
                entry_u[e] = Some(u);
            }
        }
    }
    // Straight lines: an edge alone on both sides whose cards overlap runs straight.
    for c in &g.chains {
        if c.vs.len() != 2 || side_exit[c.edge].is_some() {
            continue;
        }
        let (a, b) = (c.vs[0], c.vs[1]);
        let alone = |i: usize, out: bool| {
            g.chains
                .iter()
                .filter(|d| if out { d.vs[0] == i } else { d.vs[d.vs.len() - 1] == i })
                .count()
                == 1
        };
        let (ka, kb) = (
            input.nodes[match g.vs[a].kind {
                Kind::Real(x) => x,
                _ => continue,
            }]
            .ports,
            input.nodes[match g.vs[b].kind {
                Kind::Real(x) => x,
                _ => continue,
            }]
            .ports,
        );
        let range = |i: usize, k: Ports| {
            if k == Ports::Corners {
                (g.vs[i].u, g.vs[i].u)
            } else {
                let m = (g.vs[i].breadth / 4.0).min(12.0);
                (left(i) + m, right(i) - m)
            }
        };
        let (ra, rb) = (range(a, ka), range(b, kb));
        let (lo, hi) = (ra.0.max(rb.0), ra.1.min(rb.1));
        if lo <= hi && (alone(a, true) || ka == Ports::Corners) && (alone(b, false) || kb == Ports::Corners) {
            let mid = ((g.vs[a].u + g.vs[b].u) / 2.0).clamp(lo, hi);
            exit_u[c.edge] = Some(mid);
            entry_u[c.edge] = Some(mid);
        }
    }
    // Hops: the bends each edge makes in each gap.
    let mut hops: Vec<Hop> = Vec::new();
    for c in &g.chains {
        for (s, w) in c.vs.windows(2).enumerate() {
            let from = if s == 0 {
                exit_u[c.edge].unwrap_or(uv(w[0]))
            } else {
                uv(w[0])
            };
            let to = if s + 2 == c.vs.len() {
                entry_u[c.edge].unwrap_or(uv(w[1]))
            } else {
                uv(w[1])
            };
            hops.push(Hop {
                edge: c.edge,
                gap: g.vs[w[0]].rank,
                from,
                to,
                track: usize::MAX,
            });
        }
    }
    // Tracks: overlapping bends of one gap take different tracks, ordered to cross least; bends
    // into one point may share one.
    let mut tracks = vec![0usize; g.ranks];
    for (gap, count) in tracks.iter_mut().enumerate() {
        let ids: Vec<usize> = (0..hops.len())
            .filter(|&h| hops[h].gap == gap && (hops[h].from - hops[h].to).abs() > EPS)
            .collect();
        let span = |h: &Hop| (h.from.min(h.to), h.from.max(h.to));
        let inside = |x: f64, s: (f64, f64)| x > s.0 + EPS && x < s.1 - EPS;
        let cost = |a: &Hop, b: &Hop| (inside(b.from, span(a)) as i32) + (inside(a.to, span(b)) as i32);
        let mut keyed: Vec<(usize, i32)> = ids
            .iter()
            .map(|&i| {
                let score: i32 = ids
                    .iter()
                    .filter(|&&j| j != i)
                    .map(|&j| cost(&hops[i], &hops[j]) - cost(&hops[j], &hops[i]))
                    .sum();
                (i, score)
            })
            .collect();
        keyed.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        let mut placed: Vec<usize> = Vec::new();
        for (i, _) in keyed {
            let si = span(&hops[i]);
            let mut t = 0;
            for &j in &placed {
                let sj = span(&hops[j]);
                let overlap = si.0 < sj.1 + p.track_gap && sj.0 < si.1 + p.track_gap;
                let merge = (hops[i].to - hops[j].to).abs() < EPS
                    && input.edges[hops[i].edge].to == input.edges[hops[j].edge].to;
                if overlap && !merge {
                    t = t.max(hops[j].track + 1);
                }
            }
            hops[i].track = t;
            *count = (*count).max(t + 1);
            placed.push(i);
        }
    }
    // Rows along the flow.
    let mut depth = vec![0.0f64; g.ranks];
    for i in 0..n {
        let r = g.vs[i].rank;
        depth[r] = depth[r].max(input.nodes[i].depth);
    }
    let label_depth = input
        .edges
        .iter()
        .filter_map(|e| e.label.map(|l| l[1]))
        .fold(0.0, f64::max);
    let group_label = input.groups.iter().map(|gr| gr.label[1]).fold(0.0, f64::max);
    let ends_here = |r: usize| g.spans.iter().any(|s| s.is_some_and(|s| s.1 == r));
    let starts_here = |r: usize| g.spans.iter().any(|s| s.is_some_and(|s| s.0 == r));
    let labelled_gap = |r: usize| {
        g.chains
            .iter()
            .any(|c| c.vs.len() == 2 && g.vs[c.vs[0]].rank == r && input.edges[c.edge].label.is_some())
    };
    let lane_room = if input.lanes > 0 {
        p.lane_label[1] + p.lane_pad
    } else {
        0.0
    };
    let mut rows = Rows {
        top: vec![0.0; g.ranks],
        depth: depth.clone(),
        label_v: vec![0.0; g.ranks],
        track_v: vec![0.0; g.ranks],
        group_end_v: vec![0.0; g.ranks],
        group_start_v: vec![0.0; g.ranks],
    };
    let first_starts = starts_here(0);
    let mut v = lane_room
        + if first_starts {
            group_label / 2.0 + p.group_pad + 2.0
        } else {
            0.0
        };
    for r in 0..g.ranks {
        rows.top[r] = v;
        v += depth[r];
        if r + 1 == g.ranks {
            if ends_here(r) {
                rows.group_end_v[r] = v + p.group_pad;
                v += p.group_pad;
            }
            break;
        }
        let gap_top = v;
        let mut cursor = v + p.arrow.min(4.0);
        if ends_here(r) {
            cursor = v + p.group_pad;
            rows.group_end_v[r] = cursor;
            cursor += p.track_gap;
        }
        rows.label_v[r] = cursor;
        if labelled_gap(r) {
            cursor += label_depth + 2.0 * p.label_pad;
        }
        cursor += extra[r];
        rows.track_v[r] = cursor + p.track_gap * 0.5;
        cursor += tracks[r] as f64 * p.track_gap;
        if starts_here(r + 1) {
            cursor += group_label / 2.0 + 2.0;
            rows.group_start_v[r] = cursor;
            cursor += p.group_pad;
        } else {
            cursor += p.arrow + 4.0;
        }
        v = gap_top + (cursor - gap_top).max(p.rank_gap);
    }
    let bottom = v;
    // Card boxes.
    let node_rect = |i: usize| {
        let r = g.vs[i].rank;
        Rect::new(left(i), rows.top[r], g.vs[i].breadth, input.nodes[i].depth)
    };
    let nodes: Vec<Rect> = (0..n).map(node_rect).collect();
    // Group and lane boxes.
    let boxes = container_boxes(g, input);
    let mut groups: Vec<Rect> = Vec::new();
    for (gi, span) in g.spans.iter().enumerate() {
        match (span, boxes.0[gi]) {
            (Some((a, b)), Some((l, rt))) => {
                let top = if *a == 0 {
                    rows.top[0] - p.group_pad
                } else {
                    rows.group_start_v[a - 1]
                };
                let bot = rows.group_end_v[*b];
                groups.push(Rect::new(l - p.group_pad, top, rt - l + 2.0 * p.group_pad, bot - top));
            }
            _ => groups.push(Rect::default()),
        }
    }
    let mut lanes: Vec<Rect> = Vec::new();
    for span in &boxes.1 {
        match span {
            Some((l, rt)) => lanes.push(Rect::new(
                l - p.lane_pad - p.lane_label[0],
                0.0,
                rt - l + 2.0 * p.lane_pad + p.lane_label[0],
                bottom + p.lane_pad,
            )),
            None => lanes.push(Rect::default()),
        }
    }
    // Lines.
    let mut routes: Vec<Route> = (0..input.edges.len())
        .map(|_| Route {
            points: vec![],
            back: false,
            label: None,
            end: Side::Before,
        })
        .collect();
    for c in &g.chains {
        let e = c.edge;
        let src = input.edges[e].from;
        let dst = input.edges[e].to;
        let sr = nodes[src];
        let mut pts: Vec<[f64; 2]> = Vec::new();
        match side_exit[e] {
            Some(side) => {
                let cv = sr.y + sr.h / 2.0;
                let corner = if side == Side::Left { sr.x } else { sr.right() };
                pts.push([corner, cv]);
                pts.push([exit_u[e].unwrap_or(corner), cv]);
            }
            None => pts.push([exit_u[e].unwrap_or(sr.x + sr.w / 2.0), sr.bottom()]),
        }
        for h in hops.iter().filter(|h| h.edge == e) {
            let last = *pts.last().unwrap();
            if (h.from - h.to).abs() > EPS {
                let tv = rows.track_v[h.gap] + h.track as f64 * p.track_gap;
                pts.push([last[0], tv]);
                pts.push([h.to, tv]);
            }
            let next_rank = h.gap + 1;
            let is_last = next_rank == g.vs[dst].rank;
            if is_last {
                let dr = nodes[dst];
                let last = *pts.last().unwrap();
                pts.push([last[0], dr.y]);
            } else {
                // Through the placeholder's rank.
                let top = rows.top[next_rank];
                let last = *pts.last().unwrap();
                pts.push([last[0], top]);
                pts.push([last[0], top + rows.depth[next_rank]]);
            }
        }
        routes[e] = Route {
            points: simplify(pts),
            back: false,
            label: None,
            end: Side::Before,
        };
    }
    // Loops back up the flow, round the outside.
    let mut corridor = [0usize; 2];
    for (e, edge) in input.edges.iter().enumerate() {
        if !back[e] || edge.from == edge.to {
            continue;
        }
        let (s, t) = (nodes[edge.from], nodes[edge.to]);
        let (rs, rt) = (g.vs[edge.from].rank, g.vs[edge.to].rank);
        let outer = |side: Side, i: usize| {
            let r = g.vs[i].rank;
            !g.order[r].iter().any(|&w| {
                w != i
                    && matches!(g.vs[w].kind, Kind::Real(_) | Kind::Dummy { .. })
                    && if side == Side::Left {
                        g.vs[w].u < g.vs[i].u
                    } else {
                        g.vs[w].u > g.vs[i].u
                    }
            })
        };
        let lo = rt.min(rs);
        let hi = rt.max(rs);
        let extent = |side: Side| {
            (lo..=hi)
                .flat_map(|r| g.order[r].iter())
                .filter(|&&w| !matches!(g.vs[w].kind, Kind::Fill))
                .map(|&w| if side == Side::Left { left(w) } else { right(w) })
                .chain(
                    groups
                        .iter()
                        .filter(|b| b.w > 0.0 && b.bottom() > t.y && b.y < s.bottom())
                        .map(|b| if side == Side::Left { b.x } else { b.right() }),
                )
                .fold(
                    if side == Side::Left {
                        f64::INFINITY
                    } else {
                        f64::NEG_INFINITY
                    },
                    |a, b| if side == Side::Left { a.min(b) } else { a.max(b) },
                )
        };
        let cost = |side: Side| {
            let x = extent(side);
            let blocked = (!outer(side, edge.from)) as i32 + (!outer(side, edge.to)) as i32;
            let reach = if side == Side::Left {
                (s.x - x) + (t.x - x)
            } else {
                (x - s.right()) + (x - t.right())
            };
            (blocked, reach)
        };
        let (cl, cr) = (cost(Side::Left), cost(Side::Right));
        let side = if cl.0 < cr.0 || (cl.0 == cr.0 && cl.1 <= cr.1) {
            Side::Left
        } else {
            Side::Right
        };
        let k = corridor[if side == Side::Left { 0 } else { 1 }] as f64;
        corridor[if side == Side::Left { 0 } else { 1 }] += 1;
        let room = edge.label.map_or(0.0, |l| l[0] + 2.0 * p.label_pad) + p.arrow + 10.0;
        let blocked_ends = !outer(side, edge.from) || !outer(side, edge.to);
        let pts = if side == Side::Left {
            let x = (extent(side) - p.node_gap * 0.75 - k * p.track_gap).min(t.x - room);
            if blocked_ends {
                let below = rows.track_v.get(rs).copied().unwrap_or(s.bottom() + p.track_gap)
                    + tracks.get(rs).copied().unwrap_or(0) as f64 * p.track_gap;
                vec![
                    [s.x + s.w * 0.25, s.bottom()],
                    [s.x + s.w * 0.25, below],
                    [x, below],
                    [x, t.y + t.h / 2.0],
                    [t.x, t.y + t.h / 2.0],
                ]
            } else {
                vec![
                    [s.x, s.y + s.h / 2.0],
                    [x, s.y + s.h / 2.0],
                    [x, t.y + t.h / 2.0],
                    [t.x, t.y + t.h / 2.0],
                ]
            }
        } else {
            let x = (extent(side) + p.node_gap * 0.75 + k * p.track_gap).max(t.right() + room);
            if blocked_ends {
                let below = rows.track_v.get(rs).copied().unwrap_or(s.bottom() + p.track_gap)
                    + tracks.get(rs).copied().unwrap_or(0) as f64 * p.track_gap;
                vec![
                    [s.x + s.w * 0.75, s.bottom()],
                    [s.x + s.w * 0.75, below],
                    [x, below],
                    [x, t.y + t.h / 2.0],
                    [t.right(), t.y + t.h / 2.0],
                ]
            } else {
                vec![
                    [s.right(), s.y + s.h / 2.0],
                    [x, s.y + s.h / 2.0],
                    [x, t.y + t.h / 2.0],
                    [t.right(), t.y + t.h / 2.0],
                ]
            }
        };
        routes[e] = Route {
            points: pts,
            back: true,
            label: None,
            end: if side == Side::Left { Side::Left } else { Side::Right },
        };
    }
    // Group labels, set into the top border clear of the lines that cross it.
    let mut group_labels: Vec<Rect> = Vec::new();
    for (gi, b) in groups.iter().enumerate() {
        let [lw, lh] = input.groups[gi].label;
        if b.w <= 0.0 {
            group_labels.push(Rect::default());
            continue;
        }
        let crossing = |x0: f64, x1: f64, y: f64| {
            routes.iter().any(|r| {
                r.points.windows(2).any(|s| {
                    let (a, c) = (s[0], s[1]);
                    (a[0] - c[0]).abs() < EPS
                        && a[0] > x0 - 4.0
                        && a[0] < x1 + 4.0
                        && a[1].min(c[1]) < y
                        && a[1].max(c[1]) > y
                })
            })
        };
        let mut placed = None;
        for y in [b.y, b.bottom()] {
            let mut x = b.x + 10.0;
            while x + lw + 6.0 <= b.right() - 6.0 {
                if !crossing(x - 3.0, x + lw + 3.0, y) {
                    placed = Some(Rect::new(x, y - lh / 2.0, lw, lh));
                    break;
                }
                x += 6.0;
            }
            if placed.is_some() {
                break;
            }
        }
        group_labels.push(placed.unwrap_or(Rect::new(b.x + 10.0, b.y - lh / 2.0, lw, lh)));
    }
    // Edge labels.
    let mut taken: Vec<Rect> = group_labels.iter().filter(|r| r.w > 0.0).copied().collect();
    let mut unplaced_gaps: Vec<usize> = Vec::new();
    let segments: Vec<(usize, [f64; 2], [f64; 2])> = routes
        .iter()
        .enumerate()
        .flat_map(|(e, r)| r.points.windows(2).map(move |s| (e, s[0], s[1])))
        .collect();
    let mut borders: Vec<([f64; 2], [f64; 2])> = Vec::new();
    for b in groups.iter().filter(|b| b.w > 0.0) {
        borders.push(([b.x, b.y], [b.right(), b.y]));
        borders.push(([b.x, b.bottom()], [b.right(), b.bottom()]));
        borders.push(([b.x, b.y], [b.x, b.bottom()]));
        borders.push(([b.right(), b.y], [b.right(), b.bottom()]));
    }
    let free = |r: &Rect, e: usize, own: Option<usize>, taken: &[Rect]| -> bool {
        if nodes.iter().any(|n| n.overlaps(r, -3.0)) {
            return false;
        }
        if taken.iter().any(|t| t.overlaps(r, -4.0)) {
            return false;
        }
        for (k, (oe, a, b)) in segments.iter().enumerate() {
            if *oe == e && own.is_some_and(|o| o == k) {
                continue;
            }
            // An edge's own corners near its label are fine; its other runs are not.
            let clearance = if *oe == e { 1.0 } else { 3.0 };
            if seg_hits(*a, *b, r, clearance) {
                return false;
            }
        }
        !borders.iter().any(|(a, b)| seg_hits(*a, *b, r, 2.0))
    };
    let mut label_order: Vec<usize> = (0..input.edges.len())
        .filter(|&e| input.edges[e].label.is_some())
        .collect();
    // Decisions' exits first: "yes" and "no" sit at their corners.
    label_order.sort_by_key(|&e| (input.nodes[input.edges[e].from].ports != Ports::Corners, e));
    for e in label_order {
        let Some([lw, lh]) = input.edges[e].label else { continue };
        let route = &routes[e];
        if route.points.len() < 2 {
            continue;
        }
        let own: Vec<usize> = segments
            .iter()
            .enumerate()
            .filter(|(_, s)| s.0 == e)
            .map(|(k, _)| k)
            .collect();
        let mut cands: Vec<(Rect, Option<usize>)> = Vec::new();
        let pad = p.label_pad;
        let src = input.edges[e].from;
        let corners = input.nodes[src].ports == Ports::Corners;
        let chain_len = g.chains.iter().find(|c| c.edge == e).map_or(0, |c| c.vs.len());
        if chain_len > 2 {
            // On its own placeholder.
            if let Some(c) = g.chains.iter().find(|c| c.edge == e) {
                let mid = c.vs[(c.vs.len() - 1) / 2];
                let mid = if matches!(g.vs[mid].kind, Kind::Dummy { .. }) && g.vs[mid].breadth > 0.0 {
                    mid
                } else {
                    *c.vs
                        .iter()
                        .find(|&&w| g.vs[w].breadth > 0.0 && g.vs[w].is_dummy())
                        .unwrap_or(&mid)
                };
                let r = g.vs[mid].rank;
                let cv = rows.top[r] + rows.depth[r] / 2.0;
                let k = own.iter().copied().find(|&k| {
                    let (_, a, b) = segments[k];
                    (a[0] - b[0]).abs() < EPS
                        && (a[0] - g.vs[mid].u).abs() < 1.0
                        && a[1].min(b[1]) <= cv
                        && a[1].max(b[1]) >= cv
                });
                cands.push((Rect::new(g.vs[mid].u - lw / 2.0, cv - lh / 2.0, lw, lh), k));
            }
        }
        // A decision's answers that share its bottom corner are told apart on their own branch.
        let shared_stem = corners
            && side_exit[e].is_none()
            && input
                .edges
                .iter()
                .enumerate()
                .filter(|(k, d)| d.from == src && !back[*k] && side_exit[*k].is_none())
                .count()
                > 1;
        // Beside a shared stem, a label takes the side its siblings do not turn to.
        let sibling_right = shared_stem
            && routes.iter().zip(input.edges.iter()).enumerate().any(|(k, (r, d))| {
                k != e
                    && d.from == src
                    && r.points.len() > 2
                    && r.points
                        .windows(2)
                        .any(|w| (w[0][1] - w[1][1]).abs() < EPS && w[1][0] > w[0][0] + EPS)
            });
        let beside = |cands: &mut Vec<(Rect, Option<usize>)>, x: f64, y0: f64, y1: f64| {
            let mut y = y0 + pad;
            while y + lh <= y1 - pad + EPS {
                let right = (Rect::new(x + pad + 1.0, y, lw, lh), None);
                let left = (Rect::new(x - pad - 1.0 - lw, y, lw, lh), None);
                if sibling_right {
                    cands.push(left);
                    cands.push(right);
                } else {
                    cands.push(right);
                    cands.push(left);
                }
                y += lh / 2.0 + pad;
            }
        };
        if shared_stem || side_exit[e].is_some() {
            for &k in own.iter().skip(if side_exit[e].is_some() { 0 } else { 1 }) {
                let (_, a, b) = segments[k];
                if (a[1] - b[1]).abs() < EPS {
                    let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
                    if x1 - x0 >= lw + 2.0 * pad {
                        cands.push((Rect::new((x0 + x1) / 2.0 - lw / 2.0, a[1] - lh / 2.0, lw, lh), Some(k)));
                    }
                    cands.push((Rect::new((x0 + x1) / 2.0 - lw / 2.0, a[1] - lh - pad, lw, lh), None));
                } else {
                    beside(&mut cands, a[0], a[1].min(b[1]), a[1].max(b[1]));
                }
            }
        }
        for (idx, &k) in own.iter().enumerate() {
            let (_, a, b) = segments[k];
            let vertical = (a[0] - b[0]).abs() < EPS;
            let len = if vertical {
                (a[1] - b[1]).abs()
            } else {
                (a[0] - b[0]).abs()
            };
            if vertical {
                let x = a[0];
                let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
                // Beside the run, just past the corner it leaves: a decision's only answer down.
                if corners && idx == 0 && !shared_stem {
                    cands.push((Rect::new(x + pad + 1.0, y0 + pad, lw, lh), None));
                    cands.push((Rect::new(x - pad - 1.0 - lw, y0 + pad, lw, lh), None));
                }
                // Set into the run, in the label rows of its gap.
                let gap = (0..g.ranks).find(|&r| {
                    rows.top[r] + rows.depth[r] <= y0 + EPS && (r + 1 >= g.ranks || rows.top[r + 1] >= y1 - EPS)
                });
                if let Some(r) = gap
                    && len >= lh + 2.0 * pad
                    && !shared_stem
                {
                    let mut y = rows.label_v[r] + pad;
                    while y + lh <= y1 - pad + EPS {
                        if y >= y0 + pad - EPS {
                            cands.push((Rect::new(x - lw / 2.0, y, lw, lh), Some(k)));
                        }
                        y += lh + pad;
                    }
                }
                if len >= lh + 2.0 * pad && !(shared_stem && idx == 0) {
                    let mid = (y0 + y1) / 2.0;
                    cands.push((Rect::new(x - lw / 2.0, mid - lh / 2.0, lw, lh), Some(k)));
                }
                beside(&mut cands, x, y0, y1);
            } else {
                let y = a[1];
                let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
                if len >= lw + 2.0 * pad + if route.back { p.arrow } else { 0.0 } {
                    let cx = if route.back && idx + 1 == own.len() {
                        (x0 + x1 - if route.end == Side::Left { p.arrow } else { -p.arrow }) / 2.0
                    } else {
                        (x0 + x1) / 2.0
                    };
                    cands.push((Rect::new(cx - lw / 2.0, y - lh / 2.0, lw, lh), Some(k)));
                }
                cands.push((Rect::new((x0 + x1) / 2.0 - lw / 2.0, y - lh - pad, lw, lh), None));
                cands.push((Rect::new((x0 + x1) / 2.0 - lw / 2.0, y + pad, lw, lh), None));
            }
        }
        let found = cands.into_iter().find(|(r, k)| free(r, e, *k, &taken));
        match found {
            Some((r, _)) => {
                taken.push(r);
                routes[e].label = Some(r);
            }
            None => {
                let gap = g.vs[input.edges[e].from].rank;
                unplaced_gaps.push(gap);
            }
        }
    }
    // Extent and a shift so everything starts at zero.
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    let mut grow = |r: &Rect| {
        if r.w <= 0.0 && r.h <= 0.0 {
            return;
        }
        lo[0] = lo[0].min(r.x);
        lo[1] = lo[1].min(r.y);
        hi[0] = hi[0].max(r.right());
        hi[1] = hi[1].max(r.bottom());
    };
    for r in nodes.iter().chain(&groups).chain(&group_labels).chain(&lanes) {
        grow(r);
    }
    for r in &routes {
        for p in &r.points {
            grow(&Rect::new(p[0], p[1], 0.01, 0.01));
        }
        if let Some(l) = &r.label {
            grow(l);
        }
    }
    if !lo[0].is_finite() {
        lo = [0.0, 0.0];
        hi = [0.0, 0.0];
    }
    let shift = |r: Rect| {
        if r.w <= 0.0 && r.h <= 0.0 {
            r
        } else {
            Rect::new(r.x - lo[0], r.y - lo[1], r.w, r.h)
        }
    };
    let mut laid = Laid {
        nodes: nodes.into_iter().map(shift).collect(),
        ranks: (0..n).map(|i| g.vs[i].rank).collect(),
        edges: routes,
        groups: groups.into_iter().map(shift).collect(),
        group_labels: group_labels.into_iter().map(shift).collect(),
        lanes: lanes.into_iter().map(shift).collect(),
        extent: [hi[0] - lo[0], hi[1] - lo[1]],
        crossings: 0,
    };
    for r in &mut laid.edges {
        for p in &mut r.points {
            p[0] -= lo[0];
            p[1] -= lo[1];
        }
        if let Some(l) = r.label {
            r.label = Some(shift(l));
        }
    }
    // Lanes run the whole length of the flow.
    for l in &mut laid.lanes {
        if l.w > 0.0 {
            l.y = 0.0;
            l.h = laid.extent[1];
        }
    }
    laid.crossings = count_crossings(&laid.edges);
    unplaced_gaps.sort_unstable();
    unplaced_gaps.dedup();
    (unplaced_gaps, laid)
}

/// Drop repeated points and corners that do not turn.
fn simplify(points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for p in points {
        if let Some(last) = out.last()
            && (last[0] - p[0]).abs() < 0.01
            && (last[1] - p[1]).abs() < 0.01
        {
            continue;
        }
        if out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            let straight = ((a[0] - b[0]).abs() < 0.01 && (b[0] - p[0]).abs() < 0.01)
                || ((a[1] - b[1]).abs() < 0.01 && (b[1] - p[1]).abs() < 0.01);
            if straight {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

/// Whether the axis-parallel segment `a`–`b` passes within `clearance` of `r`.
pub(super) fn seg_hits(a: [f64; 2], b: [f64; 2], r: &Rect, clearance: f64) -> bool {
    let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
    let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
    x1 >= r.x - clearance && x0 <= r.right() + clearance && y1 >= r.y - clearance && y0 <= r.bottom() + clearance
}

/// Points where two different edges' runs cross at right angles.
pub(super) fn count_crossings(routes: &[Route]) -> usize {
    let segs: Vec<(usize, [f64; 2], [f64; 2])> = routes
        .iter()
        .enumerate()
        .flat_map(|(e, r)| r.points.windows(2).map(move |s| (e, s[0], s[1])))
        .collect();
    let mut c = 0;
    for i in 0..segs.len() {
        for j in i + 1..segs.len() {
            let (ei, a, b) = segs[i];
            let (ej, p, q) = segs[j];
            if ei == ej {
                continue;
            }
            let hi = (a[1] - b[1]).abs() < EPS;
            let hj = (p[1] - q[1]).abs() < EPS;
            if hi == hj {
                continue;
            }
            let (h0, h1, v0, v1) = if hi { (a, b, p, q) } else { (p, q, a, b) };
            let (x0, x1) = (h0[0].min(h1[0]), h0[0].max(h1[0]));
            let (y0, y1) = (v0[1].min(v1[1]), v0[1].max(v1[1]));
            if v0[0] > x0 + EPS && v0[0] < x1 - EPS && h0[1] > y0 + EPS && h0[1] < y1 - EPS {
                c += 1;
            }
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> Params {
        Params {
            node_gap: 24.0,
            rank_gap: 36.0,
            track_gap: 8.0,
            group_pad: 14.0,
            lane_pad: 10.0,
            arrow: 7.0,
            label_pad: 3.0,
            lane_label: [0.0, 0.0],
        }
    }

    fn node(b: f64) -> NodeIn {
        NodeIn {
            breadth: b,
            depth: 40.0,
            group: None,
            lane: None,
            ports: Ports::Side,
        }
    }

    fn edge(from: usize, to: usize) -> EdgeIn {
        EdgeIn { from, to, label: None }
    }

    #[test]
    fn isotonic_projection_keeps_order_and_spacing_nearest_the_wish() {
        let x = project(&[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0], &[10.0, 10.0]);
        assert_eq!(x, vec![-10.0, 0.0, 10.0]);
        let x = project(&[0.0, 100.0], &[1.0, 1.0], &[10.0]);
        assert_eq!(x, vec![0.0, 100.0]);
    }

    #[test]
    fn a_chain_runs_straight_down() {
        let input = Input {
            nodes: vec![node(100.0), node(60.0), node(140.0)],
            edges: vec![edge(0, 1), edge(1, 2)],
            groups: vec![],
            lanes: 0,
            params: params(),
        };
        let laid = layout(&input);
        for r in &laid.edges {
            assert_eq!(r.points.len(), 2, "{:?}", r.points);
            assert!((r.points[0][0] - r.points[1][0]).abs() < 0.01);
        }
        assert_eq!(laid.ranks, vec![0, 1, 2]);
    }

    #[test]
    fn a_cycle_loops_back_round_the_outside() {
        let input = Input {
            nodes: vec![node(100.0), node(100.0), node(100.0)],
            edges: vec![edge(0, 1), edge(1, 2), edge(2, 0)],
            groups: vec![],
            lanes: 0,
            params: params(),
        };
        let laid = layout(&input);
        assert!(laid.edges[2].back);
        let lo = laid.nodes.iter().map(|n| n.x).fold(f64::INFINITY, f64::min);
        let hi = laid.nodes.iter().map(|n| n.right()).fold(f64::NEG_INFINITY, f64::max);
        let xs: Vec<f64> = laid.edges[2].points.iter().map(|p| p[0]).collect();
        assert!(xs.iter().any(|&x| x < lo - 1.0) || xs.iter().any(|&x| x > hi + 1.0));
    }

    #[test]
    fn a_group_keeps_other_cards_out_of_its_box() {
        let mut nodes = vec![node(80.0), node(80.0), node(80.0), node(80.0), node(80.0)];
        nodes[1].group = Some(0);
        nodes[2].group = Some(0);
        let input = Input {
            nodes,
            edges: vec![edge(0, 1), edge(1, 2), edge(0, 3), edge(3, 4), edge(2, 4)],
            groups: vec![GroupIn { label: [60.0, 14.0] }],
            lanes: 0,
            params: params(),
        };
        let laid = layout(&input);
        let b = laid.groups[0];
        for (i, n) in laid.nodes.iter().enumerate() {
            if i == 1 || i == 2 {
                assert!(n.x >= b.x && n.right() <= b.right());
            } else {
                assert!(!n.overlaps(&b, 0.0), "{i} {n:?} {b:?}");
            }
        }
    }

    #[test]
    fn no_line_crosses_a_card() {
        let nodes: Vec<NodeIn> = (0..7).map(|_| node(90.0)).collect();
        let edges = vec![
            edge(0, 1),
            edge(0, 2),
            edge(1, 3),
            edge(2, 3),
            edge(0, 4),
            edge(4, 5),
            edge(3, 6),
            edge(5, 6),
            edge(0, 6),
        ];
        let input = Input {
            nodes,
            edges,
            groups: vec![],
            lanes: 0,
            params: params(),
        };
        let laid = layout(&input);
        for (e, r) in laid.edges.iter().enumerate() {
            let ends = (input.edges[e].from, input.edges[e].to);
            for s in r.points.windows(2) {
                for (i, n) in laid.nodes.iter().enumerate() {
                    if i == ends.0 || i == ends.1 {
                        continue;
                    }
                    assert!(
                        !seg_hits(s[0], s[1], &n.inflate(-0.5), 0.0),
                        "edge {e} crosses card {i}"
                    );
                }
            }
        }
    }
}
