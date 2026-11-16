#!/usr/bin/env node
// Builds the lockdown link map from docs/lockdown/parts/*.json and docs/lockdown/answers/*.json.
// Writes map.json, MAP.md and map.html next to this script. Zero dependencies, deterministic.
// Run: node docs/lockdown/map/build-map.mjs

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const lockdown = path.resolve(here, "..");
const partsDir = path.join(lockdown, "parts");
const answersDir = path.join(lockdown, "answers");
const APP = "http://localhost:4790";

// Edge types, strongest first. A merged edge keeps the strongest type it was seen with.
const STRENGTH = { decides: 4, conflict: 3, depends: 2, mentions: 1 };
const FIELD_TYPE = {
	conflicts: "conflict",
	proposed: "depends",
	question: "depends",
	recommendation: "depends",
	why: "mentions",
	options: "mentions",
};
const LAYOUT_WEIGHT = { decides: 3, conflict: 2.5, depends: 2, mentions: 1 };
const IMPORTANCE_ORDER = { key: 0, important: 1, detail: 2 };

// ---------- load ----------

const readJson = (file) => JSON.parse(fs.readFileSync(file, "utf8"));

const partFiles = fs
	.readdirSync(partsDir)
	.filter((f) => f.endsWith(".json"))
	.sort();
const parts = partFiles.map((f) => readJson(path.join(partsDir, f)));

const items = [];
for (const part of parts) for (const item of part.items) items.push({ ...item, part: part.id });
const byId = new Map(items.map((it) => [it.id, it]));
const byRef = new Map();
for (const it of items) if (it.refId && !byRef.has(it.refId)) byRef.set(it.refId, it.id);

const answers = new Map();
for (const f of fs
	.readdirSync(answersDir)
	.filter((f) => f.endsWith(".json"))
	.sort()) {
	const file = readJson(path.join(answersDir, f));
	for (const [itemId, a] of Object.entries(file.answers || {})) {
		if (a?.decision && byId.has(itemId)) answers.set(itemId, a.decision);
	}
}

// ---------- reference extraction ----------

const ITEM_RE = /\b(?:INT|DEL|ARC|FEA-(?:CH|RP|SF|BR)|TYP|STY|PRF|OPN)-\d+\b/g;
// Entity IDs: one prefix letter, then hyphenated upper-case segments; not preceded by a word char or hyphen.
const ENTITY_RE = /(?<![A-Za-z0-9-])[CFTSPGD](?:-[A-Z0-9]+)+/g;

function refsIn(text) {
	const out = [];
	if (!text) return out;
	for (const m of text.matchAll(ITEM_RE)) if (byId.has(m[0])) out.push(m[0]);
	for (const m of text.matchAll(ENTITY_RE)) {
		// Longest known prefix wins, so "D-FOO-BAR's" or "C-X-Y" followed by prose still resolves.
		const segs = m[0].split("-");
		for (let n = segs.length; n >= 2; n--) {
			const id = byRef.get(segs.slice(0, n).join("-"));
			if (id) {
				out.push(id);
				break;
			}
		}
	}
	return out;
}

const fieldText = (it, field) =>
	field === "options"
		? (it.options || []).map((o) => `${o.label || ""} ${o.detail || ""}`).join("\n")
		: it[field] || "";

// "answer OPN-51 first" / "answer OPN-51 and OPN-52 first" in a card's conflicts.
const DECIDE_RE = /answer\s+(OPN-\d+(?:(?:\s*,\s*|\s*,?\s+and\s+)OPN-\d+)*)\s+first/gi;
const decidedBy = new Map(); // OPN id -> Set of card ids it decides
for (const it of items) {
	for (const m of (it.conflicts || "").matchAll(DECIDE_RE)) {
		for (const opn of m[1].match(/OPN-\d+/g)) {
			if (!byId.has(opn) || opn === it.id) continue;
			if (!decidedBy.has(opn)) decidedBy.set(opn, new Set());
			decidedBy.get(opn).add(it.id);
		}
	}
}
const isCrossPart = (id) => {
	const m = /^OPN-(\d+)$/.exec(id);
	return m && +m[1] >= 50 && +m[1] <= 68;
};

// ---------- edges ----------

const edgeMap = new Map(); // "from>to" -> edge
function addEdge(from, to, type, field) {
	if (from === to) return;
	const k = `${from}>${to}`;
	const e = edgeMap.get(k);
	if (!e) {
		edgeMap.set(k, { from, to, type, field, types: [type], fields: [field] });
		return;
	}
	if (!e.types.includes(type)) e.types.push(type);
	if (!e.fields.includes(field)) e.fields.push(field);
	if (STRENGTH[type] > STRENGTH[e.type]) {
		e.type = type;
		e.field = field;
	}
}

for (const it of items) {
	for (const field of Object.keys(FIELD_TYPE)) {
		for (const to of refsIn(fieldText(it, field))) {
			let type = FIELD_TYPE[field];
			if (isCrossPart(it.id) && decidedBy.get(it.id)?.has(to)) type = "conflict";
			addEdge(it.id, to, type, field);
		}
	}
}
for (const [opn, set] of decidedBy) for (const to of set) addEdge(opn, to, "decides", "conflicts");

const edges = [...edgeMap.values()]
	.map((e) => ({
		from: e.from,
		to: e.to,
		type: e.type,
		field: e.field,
		types: [...e.types].sort((a, b) => STRENGTH[b] - STRENGTH[a]),
		fields: [...e.fields].sort(),
	}))
	.sort((a, b) => cmpId(a.from, b.from) || cmpId(a.to, b.to));

// Natural order: part file order, then item number.
function cmpId(a, b) {
	const ia = items.indexOf(byId.get(a));
	const ib = items.indexOf(byId.get(b));
	return ia - ib;
}
const order = new Map(items.map((it, i) => [it.id, i]));
function byOrder(a, b) {
	return order.get(a) - order.get(b);
}

// ---------- graph metrics ----------

const out = new Map(items.map((it) => [it.id, []]));
const inn = new Map(items.map((it) => [it.id, []]));
for (const e of edges) {
	out.get(e.from).push(e);
	inn.get(e.to).push(e);
}
const neighbours = new Map(items.map((it) => [it.id, new Set()]));
for (const e of edges) {
	neighbours.get(e.from).add(e.to);
	neighbours.get(e.to).add(e.from);
}
const degree = (id) => neighbours.get(id).size;
const weightedDegree = (id) =>
	[...out.get(id), ...inn.get(id)].reduce((s, e) => s + LAYOUT_WEIGHT[e.type], 0);

// ---------- clusters (Louvain, deterministic) ----------

const N = items.length;
const idx = new Map(items.map((it, i) => [it.id, i]));
const pairW = new Map();
for (const e of edges) {
	const a = idx.get(e.from);
	const b = idx.get(e.to);
	const k = a < b ? `${a},${b}` : `${b},${a}`;
	pairW.set(k, (pairW.get(k) || 0) + LAYOUT_WEIGHT[e.type]);
}
function louvain(n, wedges, resolution) {
	// wedges: [a, b, w] undirected, a may equal b (self loop).
	let membership = Array.from({ length: n }, (_, i) => i);
	let level = { n, wedges };
	for (let pass = 0; pass < 20; pass++) {
		const { n: ln, wedges: le } = level;
		const adj = Array.from({ length: ln }, () => new Map());
		const k = new Float64Array(ln);
		let m2 = 0;
		for (const [a, b, w] of le) {
			adj[a].set(b, (adj[a].get(b) || 0) + w);
			if (a !== b) adj[b].set(a, (adj[b].get(a) || 0) + w);
			k[a] += w;
			k[b] += w;
			m2 += 2 * w;
		}
		if (m2 === 0) break;
		const comm = Array.from({ length: ln }, (_, i) => i);
		const tot = Float64Array.from(k);
		let moved = true;
		let anyMove = false;
		for (let sweep = 0; moved && sweep < 50; sweep++) {
			moved = false;
			for (let i = 0; i < ln; i++) {
				const ci = comm[i];
				const wTo = new Map();
				for (const [j, w] of adj[i]) if (j !== i) wTo.set(comm[j], (wTo.get(comm[j]) || 0) + w);
				tot[ci] -= k[i];
				let best = ci;
				let bestGain = (wTo.get(ci) || 0) - (resolution * tot[ci] * k[i]) / m2;
				const cands = [...wTo.keys()].sort((x, y) => x - y);
				for (const c of cands) {
					const gain = wTo.get(c) - (resolution * tot[c] * k[i]) / m2;
					if (gain > bestGain + 1e-12) {
						bestGain = gain;
						best = c;
					}
				}
				tot[best] += k[i];
				if (best !== ci) {
					comm[i] = best;
					moved = true;
					anyMove = true;
				}
			}
		}
		if (!anyMove) break;
		const remap = new Map();
		for (const c of comm) if (!remap.has(c)) remap.set(c, remap.size);
		membership = membership.map((c) => remap.get(comm[c]));
		const agg = new Map();
		for (const [a, b, w] of le) {
			const ca = remap.get(comm[a]);
			const cb = remap.get(comm[b]);
			const key = ca < cb ? `${ca},${cb}` : `${cb},${ca}`;
			agg.set(key, (agg.get(key) || 0) + w);
		}
		level = {
			n: remap.size,
			wedges: [...agg.entries()].map(([key, w]) => [...key.split(",").map(Number), w]),
		};
	}
	return membership;
}
const rawComm = louvain(
	N,
	[...pairW.entries()].map(([key, w]) => [...key.split(",").map(Number), w]),
	1.0,
);

// Renumber clusters by size (desc), then by first member; singletons/isolated collapse to 0 ("unclustered").
const MIN_CLUSTER = 4;
const commMembers = new Map();
rawComm.forEach((c, i) => {
	if (!commMembers.has(c)) commMembers.set(c, []);
	commMembers.get(c).push(i);
});
const commList = [...commMembers.values()]
	.filter((m) => m.length >= MIN_CLUSTER)
	.sort((a, b) => b.length - a.length || a[0] - b[0]);
const clusterOf = new Map(items.map((it) => [it.id, 0]));
commList.forEach((members, ci) => {
	for (const i of members) clusterOf.set(items[i].id, ci + 1);
});

// Cluster descriptions from distinctive title words, dominant parts and groups.
const STOP = new Set(
	"a an and are as at be by for from in into is it its of on or the to with without what which who when where how one two every each no not only per its this that their than then same new all any own more less same before after under over via vs".split(
		" ",
	),
);
const words = (s) =>
	(s || "")
		.toLowerCase()
		.replace(/`[^`]*`/g, " ")
		.split(/[^a-z0-9]+/)
		.filter((w) => w.length > 2 && !STOP.has(w) && !/^\d+$/.test(w));
const globalDf = new Map();
for (const it of items)
	for (const w of new Set(words(it.title))) globalDf.set(w, (globalDf.get(w) || 0) + 1);
const partTitle = new Map(parts.map((p) => [p.id, p.title]));
const partShort = (id) => id.replace(/^\d+[a-d]?-/, "");

const clusters = commList.map((members, ci) => {
	const ids = members.map((i) => items[i].id);
	const tf = new Map();
	for (const id of ids)
		for (const w of new Set(words(byId.get(id).title))) tf.set(w, (tf.get(w) || 0) + 1);
	const keywords = [...tf.entries()]
		.filter(([, c]) => c >= 2)
		.map(([w, c]) => [w, c * Math.log(N / globalDf.get(w))])
		.sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1))
		.slice(0, 5)
		.map(([w]) => w);
	const count = (f) => {
		const m = new Map();
		for (const id of ids) {
			const v = f(byId.get(id));
			if (v) m.set(v, (m.get(v) || 0) + 1);
		}
		return [...m.entries()].sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1));
	};
	const partsCount = count((it) => it.part);
	const groups = count((it) => it.group).slice(0, 3);
	const inside = new Set(ids);
	const internalDeg = (id) => [...neighbours.get(id)].filter((n) => inside.has(n)).length;
	const hubs = [...ids]
		.sort((a, b) => internalDeg(b) - internalDeg(a) || degree(b) - degree(a) || byOrder(a, b))
		.slice(0, 4);
	const label = groups.length ? groups[0][0] : partTitle.get(partsCount[0][0]);
	const mix = partsCount
		.slice(0, 3)
		.map(([p, c]) => `${Math.round((100 * c) / ids.length)}% ${partShort(p)}`)
		.join(", ");
	const otherGroups = groups.slice(1).map(([g]) => g);
	const description = [
		otherGroups.length ? `also ${otherGroups.join("; ")}` : "",
		keywords.length ? `title words: ${keywords.slice(0, 4).join(", ")}` : "",
		mix,
	]
		.filter(Boolean)
		.join(" · ");
	return {
		id: ci + 1,
		label,
		size: ids.length,
		keywords,
		description,
		parts: Object.fromEntries(partsCount),
		groups: groups.map(([g, c]) => ({ group: g, count: c })),
		hubs,
		members: ids,
	};
});

// ---------- layout (seeded; clusters packed as discs, cards force-laid inside their disc) ----------

function mulberry32(seed) {
	let a = seed >>> 0;
	return () => {
		a = (a + 0x6d2b79f5) >>> 0;
		let t = a;
		t = Math.imul(t ^ (t >>> 15), t | 1);
		t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
}
const rand = mulberry32(20261026);
const X = new Float64Array(N);
const Y = new Float64Array(N);
const SPREAD = 30; // ideal distance between linked cards inside a cluster
const isCore = items.map((it) => it.importance !== "detail");

// Discs: one per cluster plus one for unclustered cards (id 0), radius from member count.
const members = new Map();
items.forEach((it, i) => {
	const c = clusterOf.get(it.id);
	if (!members.has(c)) members.set(c, []);
	members.get(c).push(i);
});
const discIds = [...members.keys()].sort((a, b) => (a === 0) - (b === 0) || a - b);
const disc = new Map(
	discIds.map((c) => [c, { r: SPREAD * 0.8 * Math.sqrt(members.get(c).length) + 16, x: 0, y: 0 }]),
);
// Pack: sunflower start, then push overlapping discs apart while pulling them to the middle.
discIds.forEach((c, i) => {
	const th = i * 2.39996323;
	const r = 260 * Math.sqrt(i);
	disc.get(c).x = r * Math.cos(th);
	disc.get(c).y = r * Math.sin(th) * 0.8;
});
// Linked clusters sit closer: inter-cluster weight pulls their discs together.
const between = new Map();
for (const [key, w] of pairW) {
	const [a, b] = key.split(",").map(Number);
	const ca = clusterOf.get(items[a].id);
	const cb = clusterOf.get(items[b].id);
	if (ca === cb || ca === 0 || cb === 0) continue;
	const k2 = ca < cb ? `${ca},${cb}` : `${cb},${ca}`;
	between.set(k2, (between.get(k2) || 0) + w);
}
const GAP = 64;
for (let it = 0; it < 900; it++) {
	const settle = it >= 600; // last pass only separates discs, so none overlap
	for (const c of settle ? [] : discIds) {
		const d = disc.get(c);
		d.x *= 0.995;
		d.y *= 0.995 * 0.996; // slightly wider than tall, to fit a landscape screen
	}
	for (const [key, w] of settle ? [] : between) {
		const [a, b] = key.split(",").map(Number);
		const A = disc.get(a);
		const B = disc.get(b);
		const dx = B.x - A.x;
		const dy = B.y - A.y;
		const d = Math.hypot(dx, dy) || 1;
		const f = (Math.min(0.02, w * 0.0004) * (d - A.r - B.r - GAP)) / d;
		A.x += dx * f;
		A.y += dy * f;
		B.x -= dx * f;
		B.y -= dy * f;
	}
	for (let p = 0; p < discIds.length; p++) {
		for (let q = p + 1; q < discIds.length; q++) {
			const A = disc.get(discIds[p]);
			const B = disc.get(discIds[q]);
			const dx = B.x - A.x;
			const dy = B.y - A.y;
			const d = Math.hypot(dx, dy) || 1;
			const min = A.r + B.r + GAP;
			if (d >= min) continue;
			const push = (min - d) / 2 / d;
			A.x -= dx * push;
			A.y -= dy * push;
			B.x += dx * push;
			B.y += dy * push;
		}
	}
}

for (let i = 0; i < N; i++) {
	const d = disc.get(clusterOf.get(items[i].id));
	const a = rand() * Math.PI * 2;
	const r = Math.sqrt(rand()) * d.r * 0.8;
	X[i] = d.x + r * Math.cos(a);
	Y[i] = d.y + r * Math.sin(a);
}
const intraEdges = [...pairW.entries()]
	.map(([key, w]) => [...key.split(",").map(Number), w])
	.filter(([a, b]) => clusterOf.get(items[a].id) === clusterOf.get(items[b].id));

function simulate(movable, active, iterations) {
	const k = SPREAD;
	const groups = discIds.map((c) => members.get(c).filter((i) => active[i]));
	const es = intraEdges.filter(([a, b]) => active[a] && active[b]);
	const dx = new Float64Array(N);
	const dy = new Float64Array(N);
	let temp = k * 2;
	for (let it = 0; it < iterations; it++) {
		dx.fill(0);
		dy.fill(0);
		for (const g of groups) {
			for (let p = 0; p < g.length; p++) {
				const i = g[p];
				for (let q = p + 1; q < g.length; q++) {
					const j = g[q];
					let ddx = X[i] - X[j];
					let ddy = Y[i] - Y[j];
					let d2 = ddx * ddx + ddy * ddy;
					if (d2 < 0.01) {
						ddx = 0.1 * (((i * 7 + j) % 5) - 2);
						ddy = 0.1;
						d2 = ddx * ddx + ddy * ddy;
					}
					const f = (k * k) / d2;
					dx[i] += ddx * f;
					dy[i] += ddy * f;
					dx[j] -= ddx * f;
					dy[j] -= ddy * f;
				}
			}
		}
		for (const [a, b, w] of es) {
			const ddx = X[a] - X[b];
			const ddy = Y[a] - Y[b];
			const d = Math.sqrt(ddx * ddx + ddy * ddy) || 0.01;
			const f = (d / k) * Math.sqrt(w) * 0.08;
			dx[a] -= ddx * f;
			dy[a] -= ddy * f;
			dx[b] += ddx * f;
			dy[b] += ddy * f;
		}
		for (let c = 0; c < discIds.length; c++) {
			const d = disc.get(discIds[c]);
			for (const i of groups[c]) {
				dx[i] += (d.x - X[i]) * 0.012;
				dy[i] += (d.y - Y[i]) * 0.012;
			}
		}
		for (let c = 0; c < discIds.length; c++) {
			const d = disc.get(discIds[c]);
			for (const i of groups[c]) {
				if (!movable[i]) continue;
				const m = Math.sqrt(dx[i] * dx[i] + dy[i] * dy[i]) || 1;
				const step = Math.min(m, temp);
				X[i] += (dx[i] / m) * step;
				Y[i] += (dy[i] / m) * step;
				// Keep every card inside its cluster's disc.
				const ox = X[i] - d.x;
				const oy = Y[i] - d.y;
				const o = Math.hypot(ox, oy);
				if (o > d.r) {
					X[i] = d.x + (ox / o) * d.r;
					Y[i] = d.y + (oy / o) * d.r;
				}
			}
		}
		temp = Math.max(k * 0.03, temp * 0.985);
	}
}
simulate(isCore, isCore, 360);
// Detail cards start next to their linked core cards in the same cluster, then settle with core fixed.
for (let i = 0; i < N; i++) {
	if (isCore[i]) continue;
	const c = clusterOf.get(items[i].id);
	const nb = [...neighbours.get(items[i].id)]
		.map((id) => idx.get(id))
		.filter((j) => isCore[j] && clusterOf.get(items[j].id) === c);
	if (!nb.length) continue;
	let sx = 0;
	let sy = 0;
	for (const j of nb) {
		sx += X[j];
		sy += Y[j];
	}
	X[i] = sx / nb.length + (rand() - 0.5) * SPREAD;
	Y[i] = sy / nb.length + (rand() - 0.5) * SPREAD;
}
simulate(
	isCore.map((c) => !c),
	items.map(() => true),
	260,
);
const discs = discIds.map((c) => {
	const d = disc.get(c);
	return { cluster: c, x: Math.round(d.x), y: Math.round(d.y), r: Math.round(d.r) };
});

// ---------- map.json ----------

const nodes = items.map((it, i) => ({
	id: it.id,
	part: it.part,
	title: it.title,
	importance: it.importance,
	kind: it.kind,
	refId: it.refId || null,
	answered: answers.get(it.id) || null,
	group: it.group || null,
	cluster: clusterOf.get(it.id),
	degree: degree(it.id),
	x: Math.round(X[i] * 10) / 10,
	y: Math.round(Y[i] * 10) / 10,
}));
const typeCounts = Object.fromEntries(
	Object.keys(STRENGTH).map((t) => [t, edges.filter((e) => e.type === t).length]),
);
const map = {
	source: "docs/lockdown/parts/*.json, docs/lockdown/answers/*.json",
	generator: "docs/lockdown/map/build-map.mjs",
	app: `${APP}/#/part/<partId>/<itemId>`,
	edgeTypes: {
		decides:
			"From an OPN-50..68 cross-part card to a card whose conflicts say 'answer OPN-xx first'",
		conflict: "Referenced in conflicts, or an OPN-50..68 card referencing a card it decides",
		depends: "Referenced in proposed, question or recommendation",
		mentions: "Referenced in why or options",
	},
	parts: parts.map((p) => ({ id: p.id, title: p.title, items: p.items.length })),
	counts: {
		nodes: nodes.length,
		edges: edges.length,
		edgesByType: typeCounts,
		answered: answers.size,
	},
	layout: { discs },
	clusters: clusters.map(({ members, ...c }) => ({ ...c, members })),
	nodes,
	edges,
};
// One node, edge or cluster per line: readable, diffable and compact.
const lines = (arr) => arr.map((x) => `\t\t${JSON.stringify(x)}`).join(",\n");
const head = Object.entries(map)
	.filter(([k]) => !["clusters", "nodes", "edges"].includes(k))
	.map(([k, v]) => `\t${JSON.stringify(k)}: ${JSON.stringify(v)}`);
fs.writeFileSync(
	path.join(here, "map.json"),
	`{\n${head.join(",\n")},\n\t"clusters": [\n${lines(map.clusters)}\n\t],\n\t"nodes": [\n${lines(nodes)}\n\t],\n\t"edges": [\n${lines(edges)}\n\t]\n}\n`,
);

// ---------- MAP.md ----------

const it_ = (id) => byId.get(id);
const ref = (id) =>
	`\`${id}\` ${it_(id).title} *(${it_(id).importance}${answers.has(id) ? `, answered: ${answers.get(id)}` : ""})*`;
const sortIds = (ids) =>
	[...new Set(ids)].sort(
		(a, b) =>
			IMPORTANCE_ORDER[it_(a).importance] - IMPORTANCE_ORDER[it_(b).importance] || byOrder(a, b),
	);
const list = (ids, cap = 8) => {
	const s = sortIds(ids);
	const shown = s.slice(0, cap).map(ref).join("; ");
	return s.length > cap ? `${shown}; and ${s.length - cap} more in map.html` : shown;
};

const md = [];
md.push("# Lockdown link map");
md.push("");
md.push(
	`Generated by \`build-map.mjs\` from the ${parts.length} part files and Daniel's answers; do not edit by hand. Each of the ${nodes.length} cards is a node, and a card links to another when its text names that card's item ID (such as \`ARC-06\`) or entity ID (such as \`D-ARTIFACT\`, resolved to the card that defines it). A link's type says where the reference sits: **decides** runs from a cross-part card (OPN-50 to OPN-68) to each card whose conflicts say "answer OPN-xx first"; **conflict** means the reference is in \`conflicts\`, or a cross-part card names a card it decides; **depends** means the reference is in \`proposed\`, \`question\` or \`recommendation\`, so the card's answer builds on the other; **mentions** means it is only in \`why\` or the options. When one card references another in several fields, the link keeps its strongest type (decides, conflict, depends, mentions). "Depends on" lists the cards a card's own answer builds on; "depended on by" lists the cards whose answers build on it, which are the ones to recheck when it changes. Open \`map.html\` for the interactive graph; each card there links to the lockdown app.`,
);
md.push("");
md.push("## Counts");
md.push("");
md.push("| Part | Cards | Key | Answered |");
md.push("| --- | ---: | ---: | ---: |");
for (const p of parts) {
	const its = items.filter((x) => x.part === p.id);
	md.push(
		`| ${p.id} ${p.title} | ${its.length} | ${its.filter((x) => x.importance === "key").length} | ${its.filter((x) => answers.has(x.id)).length} |`,
	);
}
md.push(
	`| **Total** | **${nodes.length}** | **${items.filter((x) => x.importance === "key").length}** | **${answers.size}** |`,
);
md.push("");
md.push(
	`Links: ${edges.length} in total, ${Object.entries(typeCounts)
		.map(([t, c]) => `${c} ${t}`)
		.join(", ")}. ${nodes.filter((n) => n.degree === 0).length} cards have no links.`,
);
md.push("");

md.push("## What an answer ripples into");
md.push("");
const answeredIds = [...answers.keys()].sort(byOrder);
if (!answeredIds.length) md.push("No card is answered yet.");
else {
	md.push(
		"For each answered card, the unanswered cards whose text references it, strongest link first. These may need review now that the answer is in.",
	);
	md.push("");
	for (const id of answeredIds) {
		md.push(`### \`${id}\` ${it_(id).title} — ${answers.get(id)}`);
		md.push("");
		const refs = inn.get(id).filter((e) => !answers.has(e.from));
		if (!refs.length) {
			md.push("No unanswered card references it.");
			md.push("");
			continue;
		}
		for (const t of Object.keys(STRENGTH)) {
			const ids = refs.filter((e) => e.type === t).map((e) => e.from);
			if (ids.length) md.push(`- **${t}** (${ids.length}): ${list(ids, 40)}`);
		}
		md.push("");
	}
}

md.push("## Hubs: the 20 most-linked cards");
md.push("");
md.push(
	"Ranked by distinct linked cards in either direction; in and out count links pointing at and from the card.",
);
md.push("");
md.push("| # | Card | Part | Importance | Linked cards | In | Out | Answered |");
md.push("| ---: | --- | --- | --- | ---: | ---: | ---: | --- |");
const hubs = [...items.map((x) => x.id)]
	.sort((a, b) => degree(b) - degree(a) || weightedDegree(b) - weightedDegree(a) || byOrder(a, b))
	.slice(0, 20);
hubs.forEach((id, i) => {
	const x = it_(id);
	md.push(
		`| ${i + 1} | \`${id}\` ${x.title} | ${x.part} | ${x.importance} | ${degree(id)} | ${inn.get(id).length} | ${out.get(id).length} | ${answers.get(id) || ""} |`,
	);
});
md.push("");

md.push("## Clusters");
md.push("");
md.push(
	`Densely linked groups found by Louvain community detection on the undirected graph (weights: decides 3, conflict 2.5, depends 2, mentions 1). Each cluster is named after its largest card group; the row adds its next groups, the title words that set it apart and its part mix, then its most connected cards. ${
		nodes.filter((n) => n.cluster === 0).length
	} cards in groups smaller than ${MIN_CLUSTER} (mostly unlinked) are left unclustered.`,
);
md.push("");
md.push("| Cluster | Cards | About | Most connected |");
md.push("| --- | ---: | --- | --- |");
for (const c of clusters) {
	md.push(
		`| ${c.id}. ${c.label} | ${c.size} | ${c.description} | ${c.hubs.map((h) => `\`${h}\` ${it_(h).title}`).join("; ")} |`,
	);
}
md.push("");

md.push("## Key cards and their direct links");
md.push("");
md.push(
	"Every `key` card, in part order. Lists show up to 8 cards, key first; mentions are counted only. `map.html` lists every link.",
);
md.push("");
for (const p of parts) {
	const keys = items.filter((x) => x.part === p.id && x.importance === "key");
	if (!keys.length) continue;
	md.push(`### ${p.id} ${p.title}`);
	md.push("");
	for (const x of keys) {
		const o = out.get(x.id);
		const i = inn.get(x.id);
		const lines = [];
		const add = (label, ids) =>
			ids.length && lines.push(`  - ${label} (${new Set(ids).size}): ${list(ids)}`);
		add(
			"Depends on",
			o.filter((e) => e.type === "depends").map((e) => e.to),
		);
		add(
			"Depended on by",
			i.filter((e) => e.type === "depends").map((e) => e.from),
		);
		add("Conflicts", [
			...o.filter((e) => e.type === "conflict").map((e) => e.to),
			...i.filter((e) => e.type === "conflict").map((e) => e.from),
		]);
		add(
			"Decides",
			o.filter((e) => e.type === "decides").map((e) => e.to),
		);
		add(
			"Decided by",
			i.filter((e) => e.type === "decides").map((e) => e.from),
		);
		const mo = o.filter((e) => e.type === "mentions").length;
		const mi = i.filter((e) => e.type === "mentions").length;
		if (mo || mi) lines.push(`  - Mentions ${mo} card${mo === 1 ? "" : "s"}; mentioned by ${mi}.`);
		const status = answers.has(x.id) ? ` — answered: ${answers.get(x.id)}` : "";
		md.push(`- **\`${x.id}\` ${x.title}**${status}${lines.length ? "" : " — no links"}`);
		md.push(...lines);
	}
	md.push("");
}
fs.writeFileSync(path.join(here, "MAP.md"), md.join("\n"));

// ---------- map.html ----------

const template = htmlTemplate();
const embedded = JSON.stringify(map).replace(/</g, "\\u003c");
fs.writeFileSync(
	path.join(here, "map.html"),
	template.replace("__MAP_JSON__", () => embedded),
);

console.log(
	`map: ${nodes.length} nodes, ${edges.length} edges (${Object.entries(typeCounts)
		.map(([t, c]) => `${t} ${c}`)
		.join(", ")}), ${clusters.length} clusters, ${answers.size} answered`,
);

// ---------- HTML template (client code avoids backticks and template placeholders) ----------

function htmlTemplate() {
	return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>RichView lockdown: link map</title>
<style>
:root {
	--bg: #f6f4f0; --surface: #fcfcfb; --ink: #1b1a18; --ink-2: #52514e; --ink-3: #8a8883; --line: #e3e0d9;
	--accent: #3b3fb6;
	--e-decides: #1f1f1f; --e-conflict: #d43d3d; --e-depends: #6b7a90; --e-mentions: #b9b6ad;
}
* { box-sizing: border-box; }
html, body { margin: 0; height: 100%; background: var(--bg); color: var(--ink);
	font: 13px/1.4 -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, Helvetica, Arial, sans-serif; }
#app { display: grid; grid-template-columns: 1fr 380px; grid-template-rows: auto 1fr; height: 100vh; }
header { grid-column: 1 / -1; display: flex; flex-wrap: wrap; gap: 10px 18px; align-items: center;
	padding: 10px 16px; border-bottom: 1px solid var(--line); background: var(--surface); }
header h1 { font-size: 15px; margin: 0; font-weight: 650; letter-spacing: -0.01em; }
header .counts { color: var(--ink-2); }
.ctl { display: flex; gap: 8px; align-items: center; color: var(--ink-2); }
.ctl label { display: inline-flex; gap: 4px; align-items: center; cursor: pointer; white-space: nowrap; }
.ctl b { color: var(--ink); font-weight: 600; }
#search-wrap { position: relative; }
#search { width: 260px; padding: 6px 9px; border: 1px solid var(--line); border-radius: 6px; font: inherit; background: #fff; }
#search:focus { outline: 2px solid var(--accent); outline-offset: -1px; }
#matches { position: absolute; top: 34px; left: 0; width: 420px; max-height: 60vh; overflow: auto; z-index: 5;
	background: #fff; border: 1px solid var(--line); border-radius: 6px; box-shadow: 0 6px 24px rgba(0,0,0,.08); display: none; }
#matches.open { display: block; }
button { font: inherit; padding: 5px 10px; border: 1px solid var(--line); border-radius: 6px; background: #fff; cursor: pointer; color: var(--ink); }
button:hover { border-color: var(--ink-3); }
button.on { background: var(--ink); color: #fff; border-color: var(--ink); }
#stage { position: relative; overflow: hidden; background: var(--bg); cursor: grab; }
#stage.panning { cursor: grabbing; }
svg { width: 100%; height: 100%; display: block; }
.edge { stroke-width: 1; opacity: .22; fill: none; vector-effect: non-scaling-stroke; }
.edge.decides { stroke: var(--e-decides); opacity: .45; }
.edge.conflict { stroke: var(--e-conflict); opacity: .32; }
.edge.depends { stroke: var(--e-depends); }
.edge.mentions { stroke: var(--e-mentions); stroke-dasharray: 3 3; }
.edge.x { opacity: .07; }
.edge.x.decides { opacity: .16; }
.disc { fill: #fff; fill-opacity: .7; stroke: var(--line); vector-effect: non-scaling-stroke; cursor: pointer; }
.disc:hover { stroke: var(--ink-3); }
.dlabel { fill: var(--ink-2); font-weight: 650; paint-order: stroke; stroke: var(--bg); stroke-linejoin: round; pointer-events: none; text-anchor: middle; }
svg.dim .disc, svg.dim .dlabel { opacity: .35; }
svg.dim .disc.hl, svg.dim .dlabel.hl { opacity: 1; }
.node circle { stroke: var(--surface); stroke-width: 1.2; vector-effect: non-scaling-stroke; }
.node.answered circle.ring { fill: none; stroke: var(--ink); stroke-width: 2.4; }
.node text { font-size: 10px; fill: var(--ink-2); paint-order: stroke; stroke: var(--bg); stroke-width: 3px;
	stroke-linejoin: round; pointer-events: none; display: none; }
.node.key text { display: block; font-weight: 600; fill: var(--ink); }
svg.zoomed .node text { display: block; }
.node { cursor: pointer; }
.node:hover circle.dot { stroke: var(--ink); stroke-width: 2; }
svg.dim .node:not(.hl) { opacity: .13; }
svg.dim .node:not(.hl) text { display: none; }
svg.dim .edge:not(.hl) { opacity: .035; }
svg.dim .node.hl text { display: block; }
.edge.hl { opacity: .95 !important; stroke-width: 1.8; }
.node.sel circle.dot { stroke: var(--ink); stroke-width: 3; }
.node.sel text { font-size: 12px; font-weight: 700; fill: var(--ink); }
.node.match circle.dot { stroke: var(--accent); stroke-width: 3; }
aside { border-left: 1px solid var(--line); background: var(--surface); overflow: auto; padding: 14px 16px 40px; }
aside h2 { font-size: 16px; margin: 4px 0 6px; line-height: 1.3; letter-spacing: -0.01em; }
aside h3 { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--ink-3); margin: 18px 0 6px; font-weight: 600; }
.chip { display: inline-block; font: 600 11px/1 ui-monospace, SFMono-Regular, Menlo, monospace; padding: 4px 6px; border-radius: 4px;
	background: #efece6; color: var(--ink); }
.meta { color: var(--ink-2); margin: 4px 0; }
.meta span + span::before { content: " · "; color: var(--ink-3); }
.open-link { display: inline-block; margin: 8px 0 2px; color: var(--accent); font-weight: 600; text-decoration: none; }
.open-link:hover { text-decoration: underline; }
.badge { display: inline-block; padding: 2px 7px; border-radius: 10px; font-size: 11px; font-weight: 600; background: var(--ink); color: #fff; }
ul.nb { list-style: none; margin: 0; padding: 0; }
ul.nb li { display: grid; grid-template-columns: 12px auto 1fr auto; gap: 7px; align-items: baseline; padding: 4px 4px; border-radius: 5px; cursor: pointer; }
ul.nb li:hover { background: #f1eee8; }
ul.nb .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; transform: translateY(1px); }
ul.nb .id { font: 600 11px ui-monospace, SFMono-Regular, Menlo, monospace; color: var(--ink); }
ul.nb .t { color: var(--ink-2); }
ul.nb .imp { font-size: 10px; color: var(--ink-3); }
ul.nb li.key .t { color: var(--ink); font-weight: 600; }
ul.nb li.ans .id::after { content: " \\2713"; color: var(--ink); }
ul.nb a { color: var(--ink-3); text-decoration: none; font-size: 11px; }
ul.nb a:hover { color: var(--accent); }
.legend { display: grid; grid-template-columns: 1fr 1fr; gap: 4px 10px; }
.legend div { display: flex; gap: 6px; align-items: center; color: var(--ink-2); }
.legend .sw { width: 10px; height: 10px; border-radius: 50%; flex: none; }
.legend .ln { width: 18px; height: 0; border-top: 2px solid; flex: none; }
.help { color: var(--ink-2); }
.help p { margin: 0 0 8px; }
.clusters li { grid-template-columns: auto 1fr auto; }
#zoombar { position: absolute; right: 12px; bottom: 12px; display: flex; gap: 6px; }
#hint { position: absolute; left: 12px; bottom: 12px; color: var(--ink-3); font-size: 12px; }
</style>
</head>
<body>
<div id="app">
<header>
	<h1>RichView lockdown: link map</h1>
	<span class="counts" id="counts"></span>
	<div id="search-wrap"><input id="search" type="search" placeholder="Search ID, entity ID or title" autocomplete="off"><div id="matches"></div></div>
	<div class="ctl" id="imp-ctl"><b>Importance</b>
		<label><input type="checkbox" data-imp="key" checked>key</label>
		<label><input type="checkbox" data-imp="important" checked>important</label>
		<label><input type="checkbox" data-imp="detail">detail</label>
		<button id="all-btn" type="button">Show all 745</button>
	</div>
	<div class="ctl" id="type-ctl"><b>Links</b>
		<label><input type="checkbox" data-type="decides" checked>decides</label>
		<label><input type="checkbox" data-type="conflict" checked>conflict</label>
		<label><input type="checkbox" data-type="depends" checked>depends</label>
		<label><input type="checkbox" data-type="mentions">mentions</label>
	</div>
</header>
<div id="stage">
	<svg id="svg"><g id="vp"><g id="discs"></g><g id="edges"></g><g id="nodes"></g><g id="dlabels"></g></g></svg>
	<div id="hint">Click a card to see its links. Cmd or Ctrl-click opens it in the lockdown app. Scroll to zoom, drag to pan.</div>
	<div id="zoombar"><button id="fit" type="button">Fit</button><button id="zin" type="button">+</button><button id="zout" type="button">&minus;</button></div>
</div>
<aside id="panel"></aside>
</div>
<script id="map-data" type="application/json">__MAP_JSON__</script>
<script>
(function () {
"use strict";
var MAP = JSON.parse(document.getElementById("map-data").textContent);
var APP = "${APP}";
var PALETTE = { "01": "#2a78d6", "02": "#eb6834", "03": "#1baf7a", "04a": "#4a3aa7", "04b": "#6250d6", "04c": "#8a7ee6", "04d": "#b2a9f0",
	"05": "#eda100", "06": "#e87ba4", "07": "#008300", "08": "#e34948" };
var RADIUS = { key: 9, important: 5.5, detail: 3.2 };
var TYPES = ["decides", "conflict", "depends", "mentions"];
var SVGNS = "http://www.w3.org/2000/svg";
var partColor = function (p) { return PALETTE[p.split("-")[0]] || "#999"; };
var partTitle = {}; MAP.parts.forEach(function (p) { partTitle[p.id] = p.title; });
var clusterById = {}; MAP.clusters.forEach(function (c) { clusterById[c.id] = c; });
var byId = {}; MAP.nodes.forEach(function (n) { n.links = []; byId[n.id] = n; });
MAP.edges.forEach(function (e) {
	byId[e.from].links.push({ other: e.to, dir: "out", e: e });
	byId[e.to].links.push({ other: e.from, dir: "in", e: e });
});
var state = { imp: { key: true, important: true, detail: false }, types: { decides: true, conflict: true, depends: true, mentions: false },
	sel: null, cluster: null, query: "", k: 1, tx: 0, ty: 0 };

var svg = document.getElementById("svg"), vp = document.getElementById("vp");
var gEdges = document.getElementById("edges"), gNodes = document.getElementById("nodes");
var el = function (tag, attrs) { var x = document.createElementNS(SVGNS, tag); for (var a in attrs) x.setAttribute(a, attrs[a]); return x; };
var esc = function (s) { return String(s).replace(/[&<>"]/g, function (c) { return { "&": "&amp;", "<": "&lt;", ">": "&gt;", "\\"": "&quot;" }[c]; }); };
var link = function (n) { return APP + "/#/part/" + encodeURIComponent(n.part) + "/" + encodeURIComponent(n.id); };

var gDiscs = document.getElementById("discs"), gDLabels = document.getElementById("dlabels");
MAP.layout.discs.forEach(function (d) {
	var c = clusterById[d.cluster];
	d.el = el("circle", { cx: d.x, cy: d.y, r: d.r, "class": "disc" });
	var tt = el("title", {}); tt.textContent = c ? "Cluster " + c.id + ": " + c.label + " (" + c.size + " cards)" : "Unclustered cards";
	d.el.appendChild(tt);
	d.el.addEventListener("click", function (ev) { ev.stopPropagation(); if (c) selectCluster(c.id); });
	gDiscs.appendChild(d.el);
	d.label = el("text", { x: d.x, y: d.y - d.r - 8, "class": "dlabel" });
	d.label.textContent = c ? c.id + ". " + c.label : "Unclustered";
	gDLabels.appendChild(d.label);
});
// Draw edges first (under nodes); detail before important before key so big nodes sit on top.
MAP.edges.forEach(function (e) {
	var a = byId[e.from], b = byId[e.to];
	e.el = el("line", { x1: a.x, y1: a.y, x2: b.x, y2: b.y, "class": "edge " + e.type + (a.cluster !== b.cluster ? " x" : "") });
	gEdges.appendChild(e.el);
});
var drawOrder = MAP.nodes.slice().sort(function (a, b) { return RADIUS[a.importance] - RADIUS[b.importance]; });
drawOrder.forEach(function (n) {
	var g = el("a", { href: link(n), target: "_blank", "class": "node " + n.importance + (n.answered ? " answered" : "") });
	g.setAttribute("transform", "translate(" + n.x + "," + n.y + ")");
	var t = el("title", {}); t.textContent = n.id + " \\u00b7 " + n.title + " (" + n.importance + ", " + n.part + (n.answered ? ", answered: " + n.answered : "") + ")";
	g.appendChild(t);
	if (n.answered) { n.ring = el("circle", { r: RADIUS[n.importance] + 3, "class": "ring" }); g.appendChild(n.ring); }
	n.dot = el("circle", { r: RADIUS[n.importance], fill: partColor(n.part), "class": "dot" });
	g.appendChild(n.dot);
	n.label = el("text", { x: RADIUS[n.importance] + 3, y: 3.5 }); n.label.textContent = n.id;
	g.appendChild(n.label);
	g.addEventListener("click", function (ev) {
		if (ev.metaKey || ev.ctrlKey || ev.shiftKey || ev.button === 1) return;
		ev.preventDefault(); ev.stopPropagation(); select(n.id);
	});
	n.el = g; gNodes.appendChild(g);
});

function visibleNode(n) {
	if (state.imp[n.importance]) return true;
	if (state.sel && (n.id === state.sel || state.nb[n.id])) return true;
	if (state.cluster && n.cluster === state.cluster) return true;
	return false;
}
function refresh() {
	var nb = state.nb = {};
	if (state.sel) byId[state.sel].links.forEach(function (l) { if (state.types[l.e.type]) nb[l.other] = true; });
	var q = state.query.trim().toLowerCase(), matches = {};
	if (q) MAP.nodes.forEach(function (n) { if (matchNode(n, q)) matches[n.id] = true; });
	var dim = !!(state.sel || state.cluster || q);
	svg.classList.toggle("dim", dim);
	MAP.nodes.forEach(function (n) {
		n.vis = visibleNode(n);
		n.el.style.display = n.vis ? "" : "none";
		var hl = n.id === state.sel || !!nb[n.id] || (state.cluster && n.cluster === state.cluster) || !!matches[n.id];
		n.el.classList.toggle("hl", hl);
		n.el.classList.toggle("sel", n.id === state.sel);
		n.el.classList.toggle("match", !!matches[n.id] && !state.sel);
	});
	MAP.edges.forEach(function (e) {
		var a = byId[e.from], b = byId[e.to];
		var vis = a.vis && b.vis && state.types[e.type];
		e.el.style.display = vis ? "" : "none";
		var hl = vis && ((state.sel && (e.from === state.sel || e.to === state.sel)) ||
			(state.cluster && a.cluster === state.cluster && b.cluster === state.cluster));
		e.el.classList.toggle("hl", !!hl);
	});
	MAP.layout.discs.forEach(function (d) {
		var on = (state.cluster && d.cluster === state.cluster) || (state.sel && byId[state.sel].cluster === d.cluster);
		d.el.classList.toggle("hl", !!on); d.label.classList.toggle("hl", !!on);
	});
	var shown = MAP.nodes.filter(function (n) { return n.vis; }).length;
	var shownE = MAP.edges.filter(function (e) { return e.el.style.display !== "none"; }).length;
	document.getElementById("counts").textContent = shown + " of " + MAP.nodes.length + " cards, " + shownE + " of " + MAP.edges.length + " links shown";
	document.getElementById("all-btn").classList.toggle("on", state.imp.key && state.imp.important && state.imp.detail);
	renderPanel();
}
function matchNode(n, q) {
	return n.id.toLowerCase().indexOf(q) >= 0 || (n.refId && n.refId.toLowerCase().indexOf(q) >= 0) || n.title.toLowerCase().indexOf(q) >= 0;
}

// ----- panel -----
var REL = [
	["decides", "in", "Decided by", "Cross-part cards whose answer settles this card's conflict"],
	["decides", "out", "Decides", "Cards whose conflict this card settles"],
	["conflict", "both", "Conflicts", "Contradictions named in either card's conflicts"],
	["depends", "out", "Depends on", "Cards this card's answer builds on"],
	["depends", "in", "Depended on by", "Cards whose answer builds on this one: recheck them when it changes"],
	["mentions", "out", "Mentions", ""],
	["mentions", "in", "Mentioned by", ""]
];
var IMP_ORDER = { key: 0, important: 1, detail: 2 };
function item(n, extra) {
	return '<li data-id="' + esc(n.id) + '" class="' + n.importance + (n.answered ? " ans" : "") + '">' +
		'<span class="dot" style="background:' + partColor(n.part) + '"></span>' +
		'<span class="id">' + esc(n.id) + '</span><span class="t">' + esc(n.title) + ' <span class="imp">' + n.importance + (extra || "") + '</span></span>' +
		'<a href="' + link(n) + '" target="_blank" title="Open in the lockdown app">app &#8599;</a></li>';
}
function sortNodes(list) {
	return list.sort(function (a, b) { return IMP_ORDER[a.importance] - IMP_ORDER[b.importance] || MAP.nodes.indexOf(a) - MAP.nodes.indexOf(b); });
}
var panel = document.getElementById("panel");
function renderPanel() {
	if (state.sel) return renderNode(byId[state.sel]);
	if (state.cluster) return renderCluster(clusterById[state.cluster]);
	var h = '<div class="help"><h2>How cards link</h2>' +
		"<p>Each dot is one lockdown card, coloured by part and sized by importance; a dark ring marks a card Daniel has answered. A line means one card's text names the other by item or entity ID.</p>" +
		"<p><b>Decides</b>: a cross-part card (OPN-50 to OPN-68) settles another card's conflict. <b>Conflict</b>: the reference sits in conflicts. <b>Depends</b>: it sits in the proposal, question or recommendation. <b>Mentions</b>: only in why or the options.</p>" +
		"<p>Click a card to list its links; the cards that depend on it are the ones an answer ripples into.</p></div>";
	h += "<h3>Parts</h3><div class=\\"legend\\">" + MAP.parts.map(function (p) {
		return '<div><span class="sw" style="background:' + partColor(p.id) + '"></span>' + esc(p.id.replace(/^(\\d+[a-d]?)-.*/, "$1")) + " " + esc(p.title) + "</div>";
	}).join("") + "</div>";
	h += '<h3>Marks</h3><div class="legend">' +
		'<div><span class="sw" style="width:18px;height:18px;background:#999"></span>key</div>' +
		'<div><span class="sw" style="width:11px;height:11px;background:#999"></span>important</div>' +
		'<div><span class="sw" style="width:7px;height:7px;background:#999"></span>detail</div>' +
		'<div><span class="sw" style="background:#999;box-shadow:0 0 0 2px #fff,0 0 0 4px #1b1a18"></span>answered</div>' +
		TYPES.map(function (t) { return '<div><span class="ln" style="border-color:var(--e-' + t + ")" + (t === "mentions" ? ";border-top-style:dashed" : "") + '"></span>' + t + "</div>"; }).join("") + "</div>";
	h += '<h3>Clusters</h3><ul class="nb clusters">' + MAP.clusters.map(function (c) {
		return '<li data-cluster="' + c.id + '"><span class="id">' + c.id + '</span><span class="t">' + esc(c.label) + ' <span class="imp">' + esc(c.description) + '</span></span><span class="imp">' + c.size + "</span></li>";
	}).join("") + "</ul>";
	var answered = MAP.nodes.filter(function (n) { return n.answered; });
	if (answered.length) h += '<h3>Answered (' + answered.length + ')</h3><ul class="nb">' + answered.map(function (n) { return item(n, " \\u00b7 " + n.answered); }).join("") + "</ul>";
	panel.innerHTML = h;
}
function renderNode(n) {
	var h = '<span class="chip">' + esc(n.id) + "</span>" + (n.refId ? ' <span class="chip">' + esc(n.refId) + "</span>" : "") +
		(n.answered ? ' <span class="badge">answered: ' + esc(n.answered) + "</span>" : "") +
		"<h2>" + esc(n.title) + "</h2>" +
		'<div class="meta"><span>' + esc(n.part) + " " + esc(partTitle[n.part] || "") + "</span></div>" +
		'<div class="meta"><span>' + n.importance + "</span><span>" + esc(n.kind) + "</span>" + (n.group ? "<span>" + esc(n.group) + "</span>" : "") +
		(n.cluster ? "<span>cluster " + n.cluster + "</span>" : "") + "</div>" +
		'<a class="open-link" href="' + link(n) + '" target="_blank">Open in the lockdown app &#8599;</a>';
	var total = 0;
	REL.forEach(function (r) {
		var seen = {}, list = [];
		n.links.forEach(function (l) {
			if (l.e.type !== r[0] || (r[1] !== "both" && l.dir !== r[1]) || seen[l.other]) return;
			seen[l.other] = true; list.push(byId[l.other]);
		});
		if (!list.length) return;
		total += list.length;
		var unans = r[1] !== "out" ? list.filter(function (x) { return !x.answered; }).length : 0;
		h += "<h3>" + r[2] + " (" + list.length + ")" + (n.answered && unans ? " \\u00b7 " + unans + " unanswered" : "") + "</h3>" +
			(r[3] ? '<div class="meta" style="margin-top:-2px">' + r[3] + (state.types[r[0]] ? "" : " (hidden in the graph)") + "</div>" : "") +
			'<ul class="nb">' + sortNodes(list).map(function (x) { return item(x, x.answered ? " \\u00b7 answered" : ""); }).join("") + "</ul>";
	});
	if (!total) h += '<p class="meta">No card references this one, and it references none.</p>';
	panel.innerHTML = h;
}
function renderCluster(c) {
	panel.innerHTML = '<span class="chip">cluster ' + c.id + "</span><h2>" + esc(c.label) + '</h2><div class="meta">' + esc(c.description) + "</div>" +
		"<h3>Cards (" + c.size + ')</h3><ul class="nb">' + sortNodes(c.members.map(function (id) { return byId[id]; })).map(function (x) { return item(x); }).join("") + "</ul>";
}
panel.addEventListener("click", function (ev) {
	if (ev.target.closest("a")) return;
	var li = ev.target.closest("li");
	if (!li) return;
	if (li.dataset.id) select(li.dataset.id, true);
	else if (li.dataset.cluster) selectCluster(+li.dataset.cluster);
});

function select(id, center) {
	state.sel = id; state.cluster = null; closeMatches(); refresh();
	panel.scrollTop = 0;
	if (center) centerOn([byId[id]].concat(Object.keys(state.nb).map(function (k) { return byId[k]; })));
}
function selectCluster(cid) {
	state.sel = null; state.cluster = cid; refresh();
	centerOn(MAP.nodes.filter(function (n) { return n.cluster === cid; }));
}
function clearSel() { state.sel = null; state.cluster = null; refresh(); }

// ----- search -----
var search = document.getElementById("search"), matchesEl = document.getElementById("matches");
function closeMatches() { matchesEl.classList.remove("open"); }
search.addEventListener("input", function () {
	state.query = search.value; state.sel = null; state.cluster = null; refresh();
	var q = state.query.trim().toLowerCase();
	if (!q) { closeMatches(); return; }
	var m = sortNodes(MAP.nodes.filter(function (n) { return matchNode(n, q); }));
	matchesEl.innerHTML = '<ul class="nb">' + (m.length ? m.slice(0, 60).map(function (n) { return item(n); }).join("") : '<li><span></span><span class="t">No card matches</span></li>') + "</ul>";
	matchesEl.classList.add("open");
});
search.addEventListener("keydown", function (ev) {
	if (ev.key === "Enter") { var li = matchesEl.querySelector("li[data-id]"); if (li) { search.value = ""; state.query = ""; select(li.dataset.id, true); } }
	if (ev.key === "Escape") { search.value = ""; state.query = ""; closeMatches(); refresh(); }
});
matchesEl.addEventListener("click", function (ev) {
	if (ev.target.closest("a")) return;
	var li = ev.target.closest("li[data-id]"); if (!li) return;
	search.value = ""; state.query = ""; select(li.dataset.id, true);
});

// ----- filters -----
document.querySelectorAll("[data-imp]").forEach(function (cb) {
	cb.addEventListener("change", function () { state.imp[cb.dataset.imp] = cb.checked; refresh(); });
});
document.querySelectorAll("[data-type]").forEach(function (cb) {
	cb.addEventListener("change", function () { state.types[cb.dataset.type] = cb.checked; refresh(); });
});
document.getElementById("all-btn").addEventListener("click", function () {
	var all = !(state.imp.key && state.imp.important && state.imp.detail);
	state.imp = all ? { key: true, important: true, detail: true } : { key: true, important: true, detail: false };
	document.querySelectorAll("[data-imp]").forEach(function (cb) { cb.checked = state.imp[cb.dataset.imp]; });
	refresh(); fit();
});

// ----- pan and zoom -----
function apply() {
	vp.setAttribute("transform", "translate(" + state.tx + "," + state.ty + ") scale(" + state.k + ")");
	// Keep marks and labels a constant screen size: counter-scale radii and text.
	// On screen, radii grow with zoom^0.3, so zooming in separates crowded cards without huge dots.
	var s = Math.pow(state.k, -0.7);
	MAP.nodes.forEach(function (n) {
		var r = RADIUS[n.importance] * s;
		n.dot.setAttribute("r", r);
		if (n.ring) n.ring.setAttribute("r", r + 2.2 / state.k);
		n.label.setAttribute("x", r + 3 / state.k);
		n.label.setAttribute("y", 3.5 / state.k);
		n.label.style.fontSize = (n.importance === "key" ? 11 : 10) / state.k + "px";
		n.label.style.strokeWidth = 3 / state.k + "px";
	});
	MAP.layout.discs.forEach(function (d) {
		d.label.style.fontSize = 13 / state.k + "px";
		d.label.style.strokeWidth = 6 / state.k + "px";
		d.label.setAttribute("y", d.y - d.r - 7 / state.k);
	});
	svg.classList.toggle("zoomed", state.k > 2.4);
}
function zoomAt(f, cx, cy) {
	var k = Math.max(0.15, Math.min(12, state.k * f)); f = k / state.k;
	state.tx = cx - (cx - state.tx) * f; state.ty = cy - (cy - state.ty) * f; state.k = k; apply();
}
function centerOn(list) {
	list = list.filter(Boolean);
	if (!list.length) return;
	var x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
	list.forEach(function (n) { x0 = Math.min(x0, n.x); y0 = Math.min(y0, n.y); x1 = Math.max(x1, n.x); y1 = Math.max(y1, n.y); });
	var W = svg.clientWidth, H = svg.clientHeight, pad = 60;
	var k = Math.min((W - 2 * pad) / Math.max(x1 - x0, 1), (H - 2 * pad) / Math.max(y1 - y0, 1));
	k = Math.max(0.15, Math.min(list.length === 1 ? 3 : 4, k));
	state.k = k; state.tx = W / 2 - k * (x0 + x1) / 2; state.ty = H / 2 - k * (y0 + y1) / 2; apply();
}
function fit() { centerOn(MAP.nodes.filter(function (n) { return n.vis; })); }
var stage = document.getElementById("stage");
stage.addEventListener("wheel", function (ev) {
	ev.preventDefault();
	var r = svg.getBoundingClientRect();
	zoomAt(Math.exp(-ev.deltaY * 0.0015), ev.clientX - r.left, ev.clientY - r.top);
}, { passive: false });
var drag = null;
stage.addEventListener("pointerdown", function (ev) {
	if (ev.target.closest(".node") || ev.target.closest("button")) return;
	drag = { x: ev.clientX, y: ev.clientY, tx: state.tx, ty: state.ty, moved: false }; stage.classList.add("panning");
});
window.addEventListener("pointermove", function (ev) {
	if (!drag) return;
	var dx = ev.clientX - drag.x, dy = ev.clientY - drag.y;
	if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
	state.tx = drag.tx + dx; state.ty = drag.ty + dy;
	vp.setAttribute("transform", "translate(" + state.tx + "," + state.ty + ") scale(" + state.k + ")");
});
window.addEventListener("pointerup", function (ev) {
	if (drag && !drag.moved && !ev.target.closest(".node") && !ev.target.closest("button")) clearSel();
	drag = null; stage.classList.remove("panning");
});
document.getElementById("fit").addEventListener("click", fit);
document.getElementById("zin").addEventListener("click", function () { zoomAt(1.4, svg.clientWidth / 2, svg.clientHeight / 2); });
document.getElementById("zout").addEventListener("click", function () { zoomAt(1 / 1.4, svg.clientWidth / 2, svg.clientHeight / 2); });
window.addEventListener("keydown", function (ev) { if (ev.key === "Escape" && document.activeElement !== search) clearSel(); });
window.addEventListener("resize", fit);

// Deep link into the map itself: map.html#INT-04 selects that card.
refresh(); fit();
var h = decodeURIComponent(location.hash.slice(1));
if (byId[h]) select(h, true);
})();
</script>
</body>
</html>
`;
}
