#!/usr/bin/env node
// Command-line driver for the lockdown app, for an agent running a live review with Daniel.
// Reads cards and answers through the app's HTTP API, links from docs/lockdown/map/map.json, and
// writes answers only through PUT/DELETE /api/answers. Zero dependencies; Node 20+.
// Run: node tools/lockdown-app/cli.mjs --help

import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const USAGE = `lockdown CLI: drive the RichView lockdown review through the app's API.

Usage: node tools/lockdown-app/cli.mjs <command> [args] [--json] [--url URL] [--root DIR]

Reading
  status                          Answered/total overall, per part, key cards and the
                                  cross-part group, plus decision counts.
  show <ID...> [--brief]          Full card with its answer and deep link. --brief: question,
                                  proposal, option labels, recommendation, conflict (~300 chars).
  next [-n N] [--importance L] [--part P] [--skip ID,ID]
                                  Next unanswered cards (default 5), in review order.
  list [--importance L] [--part P] [--unanswered|--answered] [--search TEXT]
                                  One line per card, in review order.
  search <text>                   Cards whose ID, refId, title, question or proposal contain
                                  every word of <text> (case-insensitive).
  open <ID>                       Print the card's deep link (navigate with the Chrome tools).

Links (from docs/lockdown/map/map.json, rebuilt first when older than a part or answer file)
  deps <ID>                       Decided by / decides / conflicts / depends on / depended on
                                  by / mentions, each with title, importance and answer state.
                                  -> this card's text names it; <- its text names this card.
  ripple <ID> [--all]             Unanswered cards (--all: every card) whose decides, conflict
                                  or depends link points at <ID>: recheck their proposals.

Writing (only through the app's API; refuses to replace an answer without --force)
  answer <ID> (--accept | --reject | --defer | --edit TEXT | --edit-file PATH | --choose OPT)
         [--note TEXT] [--dictated "QUOTE"] [--force]
                                  --choose takes an option label (exact, else a unique part of
                                  it) or a 1-based index and saves what the app's Choose saves.
                                  --dictated adds 'Dictated by Daniel in the review chat on
                                  <date>: "QUOTE". Recorded by the lockdown agent at his
                                  request.' to the note (no period after a quote that ends
                                  in one); --note text follows it. Without either, --force
                                  keeps the existing note. Prints the saved answer, then ripple.
  clear <ID> --force              Remove an answer (prints it first).

Options
  --json                          Machine-readable output.
  --url URL                       App server (default $LOCKDOWN_URL or http://127.0.0.1:4790).
  --root DIR                      Checkout holding docs/lockdown (default $LOCKDOWN_ROOT or the
                                  repo this script is in); must be the server's root for deps.
  --importance L                  key | important | detail | unrated, comma-separated.
  -h, --help                      This text.

Review order: importance (key, important, detail, unrated), then the start page's order (Part 08
cross-part cards OPN-50..68, Part 01, Part 02, then the other parts in part order), then file
order. States: open, accept, edit, chosen, reject, defer; "*stale" = the proposal changed since.
Exit codes: 0 ok, 1 error or refusal, 2 usage.`;

const IMPORTANCE_RANK = { key: 0, important: 1, detail: 2, unrated: 3 };
const LEVELS = Object.keys(IMPORTANCE_RANK);
const STRENGTH = { decides: 4, conflict: 3, depends: 2, mentions: 1 };
const CROSS = { partId: "08-open-work", group: "Cross-part contradictions" };
const CHOICE_PREFIX = "Chosen option: ";
const FLAGS = new Set(["json", "brief", "unanswered", "answered", "all", "accept", "reject", "defer", "force", "help"]);
const VALUES = new Set(["url", "root", "importance", "part", "skip", "n", "search", "edit", "edit-file", "choose", "note", "dictated"]);

class UsageError extends Error {}
class CliError extends Error {}

// ---------------------------------------------------------------------------------------------
// Arguments and context

function parseArgs(argv) {
	const opts = { skip: [] };
	const positional = [];
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		if (arg === "--") {
			positional.push(...argv.slice(i + 1));
			break;
		}
		if (arg === "-h") {
			opts.help = true;
			continue;
		}
		const match = /^(?:--([a-z-]+)|-(n))(?:=(.*))?$/s.exec(arg);
		if (!match) {
			positional.push(arg);
			continue;
		}
		const name = match[1] ?? match[2];
		if (FLAGS.has(name)) {
			if (match[3] !== undefined) throw new UsageError(`--${name} takes no value.`);
			opts[name] = true;
		} else if (VALUES.has(name)) {
			const value = match[3] ?? argv[++i];
			if (value === undefined) throw new UsageError(`--${name} needs a value.`);
			if (name === "skip") opts.skip.push(...value.split(",").map(normId).filter(Boolean));
			else opts[name] = value;
		} else {
			throw new UsageError(`Unknown option ${arg}.`);
		}
	}
	return { command: positional.shift(), positional, opts };
}

const normId = (id) => String(id).trim().toUpperCase();

function context(opts) {
	const url = (opts.url ?? process.env.LOCKDOWN_URL ?? "http://127.0.0.1:4790").replace(/\/+$/, "");
	const here = path.dirname(fileURLToPath(import.meta.url));
	const root = path.resolve(opts.root ?? process.env.LOCKDOWN_ROOT ?? path.join(here, "..", ".."));
	const port = new URL(url).port || "80";
	return { url, root, appBase: `http://localhost:${port}` };
}

// ---------------------------------------------------------------------------------------------
// API

async function api(ctx, method, pathname, body) {
	let res;
	try {
		res = await fetch(`${ctx.url}${pathname}`, {
			method,
			headers: body === undefined ? {} : { "Content-Type": "application/json" },
			body: body === undefined ? undefined : JSON.stringify(body),
		});
	} catch (error) {
		throw new CliError(`Cannot reach the lockdown app at ${ctx.url} (${error.cause?.code ?? error.message}).`);
	}
	const text = await res.text();
	let data;
	try {
		data = JSON.parse(text);
	} catch {
		data = text;
	}
	if (!res.ok) {
		const detail = Array.isArray(data?.details) ? ` ${data.details.join(" ")}` : "";
		throw new CliError(`${method} ${pathname} failed with ${res.status}: ${data?.error ?? text}${detail}`);
	}
	return data;
}

// Every card in review order, each with partId, answer and its place in the order.
async function loadCards(ctx) {
	const data = await api(ctx, "GET", "/api/items");
	const partOrder = new Map(data.parts.map((part, index) => [part.id, index]));
	const parts = new Map(data.parts.map((part) => [part.id, part]));
	const cards = data.items.map((item, index) => ({ ...item, fileIndex: index, section: sectionRank(item, partOrder) }));
	cards.sort(reviewOrder);
	return { cards, byId: new Map(cards.map((card) => [card.id, card])), parts };
}

// The start page's order: Part 08's cross-part contradictions, Part 01, Part 02, then the rest.
function sectionRank(item, partOrder) {
	if (item.partId === CROSS.partId && (item.group === CROSS.group || isCrossId(item.id))) return 0;
	const number = /^0?(\d+)/.exec(item.partId)?.[1];
	if (number === "1") return 1;
	if (number === "2") return 2;
	return 3 + (partOrder.get(item.partId) ?? 999);
}

function isCrossId(id) {
	const match = /^OPN-(\d+)$/.exec(id);
	return Boolean(match) && Number(match[1]) >= 50 && Number(match[1]) <= 68;
}

const importanceOf = (card) => card?.importance || "unrated";
const reviewOrder = (a, b) =>
	IMPORTANCE_RANK[importanceOf(a)] - IMPORTANCE_RANK[importanceOf(b)] || a.section - b.section || a.fileIndex - b.fileIndex;

function cardOrFail(store, id) {
	const card = store.byId.get(normId(id));
	if (!card) throw new CliError(`Unknown card ${normId(id)}. Try: search <text>.`);
	return card;
}

const deepLink = (ctx, card) => `${ctx.appBase}/#/part/${encodeURIComponent(card.partId)}/${encodeURIComponent(card.id)}`;

// ---------------------------------------------------------------------------------------------
// Options and answer state, as the app computes them

function plainExcerpt(source, max = 240) {
	const text = String(source)
		.replace(/^\s{0,3}(?:`{3,}|~{3,}).*$/gm, " ")
		.replace(/^\s*\|?\s*:?-{3,}:?\s*(?:\|\s*:?-{3,}:?\s*)*\|?\s*$/gm, " ")
		.replace(/^\s*\|/gm, "")
		.replace(/\|\s*$/gm, "")
		.replace(/\s\|\s/g, " · ")
		.replace(/!?\[([^\]\n]*)\]\([^)\n]*\)/g, "$1")
		.replace(/^\s{0,3}#{1,6}\s+/gm, "")
		.replace(/^\s{0,3}>\s?/gm, "")
		.replace(/^\s*(?:[-*+]|\d{1,9}[.)])\s+/gm, "")
		.replace(/\*\*|__|~~|`/g, "")
		.replace(/\s+/g, " ")
		.trim();
	return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

const optionName = (option) => option.label || plainExcerpt(option.detail, 120);
const choiceLine = (option) => `${CHOICE_PREFIX}${optionName(option)}${/[.!?…]$/.test(optionName(option)) ? "" : "."}`;
const choiceText = (option) => (option.detail ? `${choiceLine(option)}\n\n${option.detail}` : choiceLine(option));

// The option the recommendation names: it starts with that option's label as whole words; longest wins.
function recommendedIndex(card) {
	const recommendation = (card.recommendation ?? "").trim().toLowerCase();
	let best = -1;
	(card.options ?? []).forEach((option, index) => {
		const label = option.label.toLowerCase();
		const named = label && recommendation.startsWith(label) && !/[\p{L}\p{N}]/u.test(recommendation.charAt(label.length));
		if (named && (best === -1 || label.length > card.options[best].label.length)) best = index;
	});
	return best;
}

function chosenIndex(card) {
	const answer = card.answer;
	if (answer?.decision !== "edit" || typeof answer.text !== "string" || !card.options?.length) return -1;
	const first = answer.text.split("\n", 1)[0].trim();
	return card.options.findIndex((option) => choiceLine(option) === first);
}

function stateOf(card) {
	if (!card?.answer) return "open";
	const base = chosenIndex(card) !== -1 ? "chosen" : card.answer.decision;
	return card.answer.stale ? `${base}*stale` : base;
}

function chooseOption(card, wanted) {
	const options = card.options ?? [];
	if (!options.length) throw new CliError(`${card.id} has no options; use --accept, --edit, --reject or --defer.`);
	const list = options.map((option, index) => `  ${index + 1}. ${optionName(option)}`).join("\n");
	if (/^\d+$/.test(wanted.trim())) {
		const option = options[Number(wanted) - 1];
		if (!option) throw new CliError(`${card.id} has ${options.length} options:\n${list}`);
		return option;
	}
	const needle = wanted.trim().toLowerCase();
	const exact = options.filter((option) => optionName(option).toLowerCase() === needle);
	if (exact.length === 1) return exact[0];
	const partial = options.filter((option) => optionName(option).toLowerCase().includes(needle));
	if (partial.length === 1) return partial[0];
	throw new CliError(`${partial.length ? "Several" : "No"} options of ${card.id} match "${wanted}":\n${list}`);
}

// ---------------------------------------------------------------------------------------------
// Filters

function levelsOf(opts) {
	if (!opts.importance) return null;
	const levels = opts.importance.split(",").map((level) => level.trim().toLowerCase()).filter(Boolean);
	for (const level of levels) if (!LEVELS.includes(level)) throw new UsageError(`Unknown importance ${level}; use ${LEVELS.join(", ")}.`);
	return new Set(levels);
}

function partFilter(store, wanted) {
	if (!wanted) return null;
	const needle = wanted.trim().toLowerCase();
	const ids = [...store.parts.keys()];
	const match = ids.find((id) => id === needle) ?? ids.filter((id) => id.startsWith(needle) || id.replace(/^0/, "").startsWith(needle));
	if (typeof match === "string") return match;
	if (match.length === 1) return match[0];
	throw new UsageError(`${match.length ? "Ambiguous" : "Unknown"} part ${wanted}; parts: ${ids.join(", ")}.`);
}

function matchesSearch(card, text) {
	const words = text.toLowerCase().split(/\s+/).filter(Boolean);
	const hay = [card.id, card.refId, card.title, card.question, card.proposed].join("\n").toLowerCase();
	return words.every((word) => hay.includes(word));
}

function filterCards(store, opts) {
	const levels = levelsOf(opts);
	const part = partFilter(store, opts.part);
	if (opts.answered && opts.unanswered) throw new UsageError("Use --answered or --unanswered, not both.");
	return store.cards.filter(
		(card) =>
			(!levels || levels.has(importanceOf(card))) &&
			(!part || card.partId === part) &&
			(!opts.unanswered || !card.answer) &&
			(!opts.answered || card.answer) &&
			(!opts.search || matchesSearch(card, opts.search)),
	);
}

// ---------------------------------------------------------------------------------------------
// Text output

const pad = (value, width) => String(value).padEnd(width);
const IMP_SHORT = { key: "key", important: "imp", detail: "det", unrated: "-" };

function cardLine(card) {
	return `${pad(card.id, 10)} ${pad(IMP_SHORT[importanceOf(card)], 3)} ${pad(stateOf(card), 7)} ${pad(card.partId, 21)} ${card.title}`;
}

function compact(ctx, card) {
	return {
		id: card.id,
		partId: card.partId,
		importance: importanceOf(card),
		kind: card.kind,
		state: stateOf(card),
		title: card.title,
		link: deepLink(ctx, card),
	};
}

const indent = (text, prefix = "  ") =>
	String(text)
		.split("\n")
		.map((line) => (line ? prefix + line : line))
		.join("\n");
const cut = (text, max) => (text.length > max ? `${text.slice(0, max - 1).trimEnd()}…` : text);

function answerLines(answer) {
	if (!answer) return ["Answer: none"];
	const lines = [`Answer: ${answer.decision}${answer.stale ? " (stale: the proposal changed since)" : ""}, updated ${answer.updatedAt ?? "?"}`];
	if (answer.text) lines.push("Text:", indent(answer.text));
	if (answer.note) lines.push("Note:", indent(answer.note));
	return lines;
}

function optionLines(card, withDetail) {
	const recommended = recommendedIndex(card);
	const chosen = chosenIndex(card);
	return card.options.map((option, index) => {
		const marks = [index === recommended ? "recommended" : "", index === chosen ? "chosen" : ""].filter(Boolean);
		const head = `  ${index + 1}. ${optionName(option)}${marks.length ? ` [${marks.join(", ")}]` : ""}`;
		return withDetail && option.detail && option.label ? `${head}\n${indent(option.detail, "     ")}` : head;
	});
}

function showText(ctx, card, brief) {
	const head = `${card.id} · ${card.title}\n${card.partId} · ${importanceOf(card)} · ${card.kind}${card.refId ? ` · ${card.refId}` : ""}${brief ? "" : ` · confidence ${card.confidence}`} · ${stateOf(card)}`;
	const out = [head];
	const section = (label, text) => {
		if (text) out.push(`${label}:`, indent(text));
	};
	section("Question", card.question);
	if (!brief) section("Why", card.why);
	section("Proposed", card.proposed);
	if (card.options?.length) out.push("Options:", ...optionLines(card, !brief));
	section("Recommendation", card.recommendation);
	section("Conflicts", brief && card.conflicts ? cut(card.conflicts, 300) : card.conflicts);
	if (!brief && card.sources?.length) {
		out.push("Sources:", ...card.sources.map((source) => `  - ${source.label || "(no label)"}: ${source.ref}${source.date ? ` (${source.date})` : ""}`));
	}
	if (brief) {
		if (card.answer) out.push(`Answer: ${stateOf(card)}`);
	} else {
		out.push(...answerLines(card.answer));
	}
	out.push(`Link: ${deepLink(ctx, card)}`);
	return out.join("\n");
}

function print(opts, json, text) {
	process.stdout.write(`${opts.json ? JSON.stringify(json, null, 2) : text}\n`);
}

// ---------------------------------------------------------------------------------------------
// Map

const mapPath = (ctx) => path.join(ctx.root, "docs", "lockdown", "map", "map.json");

function newestInput(ctx) {
	let newest = 0;
	for (const dir of ["parts", "answers"]) {
		const full = path.join(ctx.root, "docs", "lockdown", dir);
		let names = [];
		try {
			names = readdirSync(full);
		} catch {
			continue;
		}
		for (const name of names) if (name.endsWith(".json")) newest = Math.max(newest, statSync(path.join(full, name)).mtimeMs);
	}
	return newest;
}

// map.json, rebuilt first when a part or answer file is newer. A failed rebuild (say, a part file
// being written) keeps the existing map and says so on stderr.
function loadMap(ctx) {
	const file = mapPath(ctx);
	let mtime = 0;
	try {
		mtime = statSync(file).mtimeMs;
	} catch {}
	if (mtime < newestInput(ctx)) {
		const script = path.join(path.dirname(file), "build-map.mjs");
		try {
			execFileSync(process.execPath, [script], { cwd: ctx.root, stdio: ["ignore", "ignore", "pipe"] });
		} catch (error) {
			const reason = String(error.stderr ?? error.message).trim().split("\n").slice(-1)[0];
			if (!mtime) throw new CliError(`Cannot build the map with ${script}: ${reason}`);
			process.stderr.write(`warning: map rebuild failed, using the existing map.json (${reason})\n`);
		}
	}
	try {
		return JSON.parse(readFileSync(file, "utf8"));
	} catch (error) {
		throw new CliError(`Cannot read ${file}: ${error.message}`);
	}
}

// A linked card as one record: live title, importance and answer from the API, else the map's node.
function linked(ctx, store, nodes, id, edge, direction) {
	const card = store.byId.get(id);
	const node = nodes.get(id);
	return {
		id,
		type: edge.type,
		direction,
		partId: card?.partId ?? node?.part ?? "",
		importance: card ? importanceOf(card) : node?.importance || "unrated",
		state: card ? stateOf(card) : node?.answered || "open",
		answered: card ? Boolean(card.answer) : Boolean(node?.answered),
		title: card?.title ?? node?.title ?? "(not in the app)",
		link: card ? deepLink(ctx, card) : undefined,
	};
}

const linkOrder = (store) => (a, b) =>
	IMPORTANCE_RANK[a.importance] - IMPORTANCE_RANK[b.importance] ||
	STRENGTH[b.type] - STRENGTH[a.type] ||
	(store.byId.get(a.id)?.section ?? 99) - (store.byId.get(b.id)?.section ?? 99) ||
	(store.byId.get(a.id)?.fileIndex ?? 0) - (store.byId.get(b.id)?.fileIndex ?? 0);

function linkLine(entry, arrows) {
	const arrow = arrows ? `${entry.direction === "out" ? "->" : "<-"} ` : "";
	return `  ${arrow}${pad(entry.id, 10)} ${pad(IMP_SHORT[entry.importance], 3)} ${pad(entry.state, 7)} ${entry.title}`;
}

function computeDeps(ctx, store, id) {
	const map = loadMap(ctx);
	const nodes = new Map(map.nodes.map((node) => [node.id, node]));
	if (!nodes.has(id)) throw new CliError(`${id} is not in map.json; rebuild it with node docs/lockdown/map/build-map.mjs.`);
	const groups = { decidedBy: [], decides: [], conflicts: [], dependsOn: [], dependedOnBy: [], mentions: [] };
	for (const edge of map.edges) {
		const out = edge.from === id;
		if (!out && edge.to !== id) continue;
		const other = out ? edge.to : edge.from;
		const entry = linked(ctx, store, nodes, other, edge, out ? "out" : "in");
		if (edge.type === "decides") groups[out ? "decides" : "decidedBy"].push(entry);
		else if (edge.type === "conflict") groups.conflicts.push(entry);
		else if (edge.type === "depends") groups[out ? "dependsOn" : "dependedOnBy"].push(entry);
		else groups.mentions.push(entry);
	}
	for (const list of Object.values(groups)) list.sort(linkOrder(store));
	return groups;
}

function computeRipple(ctx, store, id, all) {
	const map = loadMap(ctx);
	const nodes = new Map(map.nodes.map((node) => [node.id, node]));
	if (!nodes.has(id)) throw new CliError(`${id} is not in map.json; rebuild it with node docs/lockdown/map/build-map.mjs.`);
	return map.edges
		.filter((edge) => edge.to === id && edge.type !== "mentions")
		.map((edge) => linked(ctx, store, nodes, edge.from, edge, "in"))
		.filter((entry) => all || !entry.answered)
		.sort(linkOrder(store));
}

function rippleText(id, cards, all) {
	if (!cards.length) return `Ripple of ${id}: no ${all ? "" : "unanswered "}card references it through decides, conflict or depends.`;
	return [`Ripple of ${id}: ${cards.length} ${all ? "" : "unanswered "}card${cards.length === 1 ? "" : "s"} to recheck`, ...cards.map((entry) => `  ${pad(entry.type, 8)} ${linkLine(entry, false).trimStart()}`)].join("\n");
}

// ---------------------------------------------------------------------------------------------
// Commands

async function cmdStatus(ctx, opts) {
	const data = await api(ctx, "GET", "/api/parts");
	const store = await loadCards(ctx);
	const cross = store.cards.filter((card) => card.section === 0);
	const key = store.cards.filter((card) => importanceOf(card) === "key");
	const totals = { itemCount: 0, answeredCount: 0, staleCount: 0, decisions: { accept: 0, edit: 0, reject: 0, defer: 0 } };
	for (const part of data.parts) {
		totals.itemCount += part.itemCount ?? 0;
		totals.answeredCount += part.answeredCount ?? 0;
		totals.staleCount += part.staleCount ?? 0;
		for (const [decision, count] of Object.entries(part.decisions ?? {})) totals.decisions[decision] = (totals.decisions[decision] ?? 0) + count;
	}
	const chosen = store.cards.filter((card) => stateOf(card).startsWith("chosen")).length;
	const result = {
		totals: { ...totals, chosen },
		key: { itemCount: key.length, answeredCount: key.filter((card) => card.answer).length },
		crossPart: { itemCount: cross.length, answeredCount: cross.filter((card) => card.answer).length },
		parts: data.parts.map((part) => ({
			id: part.id,
			title: part.title,
			status: part.status,
			itemCount: part.itemCount,
			answeredCount: part.answeredCount,
			key: part.importance?.key ?? { itemCount: 0, answeredCount: 0 },
			decisions: part.decisions,
			staleCount: part.staleCount,
			error: part.error ?? null,
		})),
	};
	const decisions = (counts) =>
		Object.entries(counts ?? {})
			.filter(([, count]) => count)
			.map(([decision, count]) => `${decision} ${count}`)
			.join(", ") || "-";
	const lines = [
		`Answered ${totals.answeredCount}/${totals.itemCount} · key ${result.key.answeredCount}/${result.key.itemCount} · cross-part (OPN-50..68) ${result.crossPart.answeredCount}/${result.crossPart.itemCount}`,
		`Decisions: ${decisions(totals.decisions)}${chosen ? ` (${chosen === 1 ? "1 edit is an option choice" : `${chosen} edits are option choices`})` : ""}${totals.staleCount ? ` · stale ${totals.staleCount}` : ""}`,
		"",
		`${pad("part", 22)} ${pad("answered", 9)} ${pad("key", 7)} decisions`,
		...result.parts.map(
			(part) =>
				`${pad(part.id, 22)} ${pad(`${part.answeredCount}/${part.itemCount}`, 9)} ${pad(`${part.key.answeredCount}/${part.key.itemCount}`, 7)} ${part.error ? `unavailable: ${part.error}` : decisions(part.decisions)}${part.staleCount ? ` · stale ${part.staleCount}` : ""}`,
		),
	];
	print(opts, result, lines.join("\n"));
}

async function cmdShow(ctx, opts, ids) {
	if (!ids.length) throw new UsageError("show needs at least one card ID.");
	const store = await loadCards(ctx);
	const cards = ids.map((id) => cardOrFail(store, id));
	const json = cards.map((card) => ({
		id: card.id,
		partId: card.partId,
		importance: importanceOf(card),
		kind: card.kind,
		refId: card.refId,
		component: card.component,
		group: card.group,
		confidence: card.confidence,
		title: card.title,
		question: card.question,
		...(opts.brief ? {} : { why: card.why }),
		proposed: card.proposed,
		options: (card.options ?? []).map((option, index) => ({
			label: optionName(option),
			...(opts.brief ? {} : { detail: option.detail }),
			recommended: index === recommendedIndex(card),
			chosen: index === chosenIndex(card),
		})),
		recommendation: card.recommendation,
		conflicts: opts.brief && card.conflicts ? cut(card.conflicts, 300) : card.conflicts,
		...(opts.brief ? {} : { sources: card.sources }),
		state: stateOf(card),
		answer: card.answer,
		link: deepLink(ctx, card),
	}));
	print(opts, json, cards.map((card) => showText(ctx, card, opts.brief)).join("\n\n---\n\n"));
}

async function cmdNext(ctx, opts) {
	const count = opts.n === undefined ? 5 : Number(opts.n);
	if (!Number.isInteger(count) || count < 1) throw new UsageError("-n needs a positive whole number.");
	const store = await loadCards(ctx);
	const skip = new Set(opts.skip);
	const cards = filterCards(store, { ...opts, unanswered: true, answered: false }).filter((card) => !skip.has(card.id));
	const shown = cards.slice(0, count);
	const text = shown.length ? [...shown.map(cardLine), `(${cards.length} unanswered match)`].join("\n") : "No unanswered card matches.";
	print(opts, shown.map((card) => compact(ctx, card)), text);
}

async function cmdList(ctx, opts, search) {
	const store = await loadCards(ctx);
	const cards = filterCards(store, search === undefined ? opts : { ...opts, search });
	print(opts, cards.map((card) => compact(ctx, card)), cards.length ? [...cards.map(cardLine), `(${cards.length} cards)`].join("\n") : "No card matches.");
}

async function cmdDeps(ctx, opts, ids) {
	if (ids.length !== 1) throw new UsageError("deps needs one card ID.");
	const store = await loadCards(ctx);
	const card = cardOrFail(store, ids[0]);
	const groups = computeDeps(ctx, store, card.id);
	const labels = { decidedBy: "Decided by", decides: "Decides", conflicts: "Conflicts", dependsOn: "Depends on", dependedOnBy: "Depended on by", mentions: "Mentions" };
	const lines = [`${card.id} ${card.title} (${importanceOf(card)}, ${stateOf(card)})`];
	for (const [name, list] of Object.entries(groups)) {
		if (!list.length) continue;
		const arrows = name === "conflicts" || name === "mentions";
		lines.push(`${labels[name]} (${list.length})`, ...list.map((entry) => linkLine(entry, arrows)));
	}
	if (lines.length === 1) lines.push("No links.");
	print(opts, { id: card.id, title: card.title, state: stateOf(card), ...groups }, lines.join("\n"));
}

async function cmdRipple(ctx, opts, ids) {
	if (ids.length !== 1) throw new UsageError("ripple needs one card ID.");
	const store = await loadCards(ctx);
	const card = cardOrFail(store, ids[0]);
	const cards = computeRipple(ctx, store, card.id, opts.all);
	print(opts, { id: card.id, cards }, rippleText(card.id, cards, opts.all));
}

function today() {
	const now = new Date();
	return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
}

async function cmdAnswer(ctx, opts, ids) {
	if (ids.length !== 1) throw new UsageError("answer needs one card ID.");
	const modes = ["accept", "reject", "defer", "edit", "edit-file", "choose"].filter((mode) => opts[mode] !== undefined);
	if (modes.length !== 1) throw new UsageError("answer needs exactly one of --accept, --reject, --defer, --edit, --edit-file, --choose.");
	const store = await loadCards(ctx);
	const card = cardOrFail(store, ids[0]);
	const previous = card.answer;
	const mode = modes[0];
	const body = { decision: mode === "edit-file" || mode === "choose" ? "edit" : mode };
	let notice = "";
	if (mode === "edit") body.text = opts.edit;
	if (mode === "edit-file") {
		try {
			body.text = readFileSync(opts["edit-file"], "utf8");
		} catch (error) {
			throw new CliError(`Cannot read ${opts["edit-file"]}: ${error.message}`);
		}
	}
	if (mode === "choose") body.text = choiceText(chooseOption(card, opts.choose));
	if (body.decision === "edit" && !body.text.trim()) throw new UsageError("An edit needs non-empty text.");
	if (mode === "accept" && card.options?.length) {
		const recommended = recommendedIndex(card);
		notice = recommended === -1 ? "Accept keeps the proposal as written (no option is recommended)." : `Accept takes the recommended option: ${optionName(card.options[recommended])}.`;
	}
	const noteParts = [];
	if (opts.dictated !== undefined) {
		const quote = opts.dictated.trim();
		const stop = /[.!?…]$/.test(quote) ? "" : ".";
		noteParts.push(`Dictated by Daniel in the review chat on ${today()}: "${quote}"${stop} Recorded by the lockdown agent at his request.`);
	}
	if (opts.note !== undefined && opts.note.trim()) noteParts.push(opts.note.trim());
	body.note = noteParts.length ? noteParts.join("\n\n") : (previous?.note ?? "");

	if (previous && !opts.force) {
		const text = [`Refused: ${card.id} already has an answer. Pass --force to replace it.`, ...answerLines(previous)].join("\n");
		if (opts.json) print(opts, { refused: true, id: card.id, existing: previous }, "");
		else process.stderr.write(`${text}\n`);
		return 1;
	}
	const before = previous ? [`Replacing the existing answer of ${card.id}:`, ...answerLines(previous), ""] : [];
	const saved = await api(ctx, "PUT", `/api/answers/${encodeURIComponent(card.partId)}/${encodeURIComponent(card.id)}`, body);
	const after = await loadCards(ctx);
	const cards = computeRipple(ctx, after, card.id, false);
	const savedCard = after.byId.get(card.id) ?? { ...card, answer: saved.answer };
	const lines = [...before, `Saved ${card.id} (${card.partId}) as ${stateOf(savedCard)}.`, ...(notice ? [notice] : []), ...answerLines(saved.answer), `Link: ${deepLink(ctx, card)}`, "", rippleText(card.id, cards, false)];
	print(opts, { id: card.id, partId: card.partId, previous: previous ?? null, answer: saved.answer, state: stateOf(savedCard), ripple: cards }, lines.join("\n"));
	return 0;
}

async function cmdClear(ctx, opts, ids) {
	if (ids.length !== 1) throw new UsageError("clear needs one card ID.");
	if (!opts.force) throw new UsageError("clear removes Daniel's answer; pass --force to confirm.");
	const store = await loadCards(ctx);
	const card = cardOrFail(store, ids[0]);
	if (!card.answer) {
		print(opts, { id: card.id, cleared: false, previous: null }, `${card.id} has no answer; nothing to clear.`);
		return 0;
	}
	await api(ctx, "DELETE", `/api/answers/${encodeURIComponent(card.partId)}/${encodeURIComponent(card.id)}`);
	print(opts, { id: card.id, cleared: true, previous: card.answer }, [`Cleared ${card.id}. It held:`, ...answerLines(card.answer)].join("\n"));
	return 0;
}

async function cmdOpen(ctx, opts, ids) {
	if (ids.length !== 1) throw new UsageError("open needs one card ID.");
	const store = await loadCards(ctx);
	const card = cardOrFail(store, ids[0]);
	print(opts, { id: card.id, link: deepLink(ctx, card) }, deepLink(ctx, card));
}

async function main(argv) {
	const { command, positional, opts } = parseArgs(argv);
	if (opts.help || !command || command === "help") {
		process.stdout.write(`${USAGE}\n`);
		return command || opts.help ? 0 : 2;
	}
	const ctx = context(opts);
	switch (command) {
		case "status":
			return cmdStatus(ctx, opts);
		case "show":
			return cmdShow(ctx, opts, positional);
		case "next":
			return cmdNext(ctx, opts);
		case "list":
			return cmdList(ctx, opts);
		case "search":
			if (!positional.length) throw new UsageError("search needs text.");
			return cmdList(ctx, opts, positional.join(" "));
		case "deps":
			return cmdDeps(ctx, opts, positional);
		case "ripple":
			return cmdRipple(ctx, opts, positional);
		case "answer":
			return cmdAnswer(ctx, opts, positional);
		case "clear":
			return cmdClear(ctx, opts, positional);
		case "open":
			return cmdOpen(ctx, opts, positional);
		default:
			throw new UsageError(`Unknown command ${command}.`);
	}
}

main(process.argv.slice(2)).then(
	(code) => {
		process.exitCode = typeof code === "number" ? code : 0;
	},
	(error) => {
		if (error instanceof UsageError) {
			process.stderr.write(`${error.message}\nRun with --help for usage.\n`);
			process.exitCode = 2;
		} else {
			process.stderr.write(`${error instanceof CliError ? error.message : error.stack}\n`);
			process.exitCode = 1;
		}
	},
);
