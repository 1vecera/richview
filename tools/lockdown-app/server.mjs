#!/usr/bin/env node
// Local review app for the RichView lockdown: serves the UI, reads part files and guide documents
// fresh on every request and persists Daniel's answers. Contract: docs/lockdown/README.md. Usage:
// README.md here.
import { execFile } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { createReadStream } from "node:fs";
import {
	appendFile,
	mkdir,
	open,
	readFile,
	readdir,
	realpath,
	rename,
	rm,
	stat,
} from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";
import { parseArgs, promisify } from "node:util";

const DECISIONS = new Set(["accept", "edit", "reject", "defer"]);
// An item's optional importance, most important first; an item without one is "unrated".
const IMPORTANCE = ["key", "important", "detail"];
const IMPORTANCE_LEVELS = [...IMPORTANCE, "unrated"];
const ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const LOOPBACK_HOST = /^(?:localhost|127\.0\.0\.1|\[::1\])(?::\d{1,5})?$/i;
const TECH_DIR = "docs/how-it-works";
const IMAGE_DIRS = ["docs/lockdown/screenshots", "docs/guide", `${TECH_DIR}/diagrams`];
const IMAGE_TYPES = {
	".avif": "image/avif",
	".gif": "image/gif",
	".jpeg": "image/jpeg",
	".jpg": "image/jpeg",
	".png": "image/png",
	".svg": "image/svg+xml",
	".webp": "image/webp",
};
const STATIC_TYPES = {
	".css": "text/css; charset=utf-8",
	".html": "text/html; charset=utf-8",
	".js": "text/javascript; charset=utf-8",
};
const MAX_BODY_BYTES = 4 * 1024 * 1024;
const MAX_FIELD_CHARS = 200_000;
const MAX_BATCH = 5000;
// Documents the Guide view may read: these lockdown files and any Markdown file under docs/guide.
const LOCKDOWN_DOCS = ["README", "GOALS", "RULES", "BOUNDARIES", "EVIDENCE", "DELIVERY", "ids"].map(
	(name) => `docs/lockdown/${name}.md`,
);
const GUIDE_DIR = "docs/guide";
const MAX_DOC_BYTES = 4 * 1024 * 1024;
const MAX_GUIDE_DEPTH = 6;
const MAX_GUIDE_DOCS = 1000;
// Repo paths the UI may link: the evidence commit's tree, and the folders that live only on the
// lockdown branch. The UI links a path only when it is listed here.
const PINNED_COMMIT = "b4db233";
const LOCKDOWN_TREES = ["docs/lockdown", "docs/guide", "tools/lockdown-app"];
const MAX_LOCKDOWN_DEPTH = 8;
const MAX_LOCKDOWN_PATHS = 5000;
const SECURITY_HEADERS = {
	"Cross-Origin-Resource-Policy": "same-origin",
	"Referrer-Policy": "no-referrer",
	"X-Content-Type-Options": "nosniff",
	"X-Frame-Options": "DENY",
};
const PAGE_CSP =
	"default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
const SVG_CSP = "default-src 'none'; style-src 'unsafe-inline'; img-src data:; sandbox";

const USAGE = "Usage: node tools/lockdown-app/server.mjs [--port 4790] [--root <repo root>]";

let options;
try {
	options = parseArgs({
		options: {
			port: { type: "string", default: "4790" },
			root: { type: "string" },
			help: { type: "boolean", short: "h" },
		},
	}).values;
} catch (error) {
	console.error(`${error.message}\n${USAGE}`);
	process.exit(2);
}
if (options.help) {
	console.log(USAGE);
	process.exit(0);
}
const port = Number(options.port);
if (!Number.isInteger(port) || port < 0 || port > 65535) {
	console.error(`Invalid --port ${options.port}.\n${USAGE}`);
	process.exit(2);
}

const root = path.resolve(options.root ?? path.join(import.meta.dirname, "..", ".."));
const publicDir = path.join(import.meta.dirname, "public");
const partsDir = path.join(root, "docs", "lockdown", "parts");
const answersDir = path.join(root, "docs", "lockdown", "answers");
const logFile = path.join(answersDir, "log.jsonl");

class HttpError extends Error {
	constructor(status, message, details) {
		super(message);
		this.status = status;
		this.details = details;
	}
}

// Writes to one file run one after another; different files proceed independently.
const queues = new Map();
function withLock(key, task) {
	const result = (queues.get(key) ?? Promise.resolve()).then(task);
	const tail = result.then(
		() => {},
		() => {},
	);
	queues.set(key, tail);
	tail.then(() => {
		if (queues.get(key) === tail) queues.delete(key);
	});
	return result;
}

const server = createServer((req, res) => {
	route(req, res).catch((error) => {
		const status = error instanceof HttpError ? error.status : 500;
		if (status === 500) console.error(error);
		if (res.headersSent) {
			res.destroy();
			return;
		}
		sendJson(res, status, { error: error.message, ...(error.details ? { details: error.details } : {}) });
	});
});

server.on("error", (error) => {
	console.error(error.code === "EADDRINUSE" ? `Port ${port} is in use; pass --port <free port>.` : error.message);
	process.exit(1);
});

server.listen(port, "127.0.0.1", () => {
	console.log(`RichView lockdown app: http://127.0.0.1:${server.address().port}/`);
	console.log(`Root: ${root}`);
	stat(partsDir).catch(() => console.log("No docs/lockdown/parts/ under the root yet; parts appear once agents write them."));
});

for (const signal of ["SIGINT", "SIGTERM"]) {
	process.on(signal, () => {
		server.close();
		setTimeout(() => process.exit(0), 3000).unref();
		Promise.allSettled([...queues.values()]).then(() => process.exit(0));
	});
}

async function route(req, res) {
	if (!LOOPBACK_HOST.test(req.headers.host ?? "")) {
		throw new HttpError(403, "Forbidden: the Host header must be localhost or 127.0.0.1.");
	}
	const method = req.method ?? "GET";
	const isRead = method === "GET" || method === "HEAD";
	if (!isRead && !isSameOrigin(req)) throw new HttpError(403, "Forbidden: cross-origin write.");
	const url = new URL(req.url ?? "/", "http://localhost");
	if (url.pathname.startsWith("/api/")) return routeApi(req, res, method, url);
	if (!isRead) throw new HttpError(405, "Method not allowed.");
	if (url.pathname.startsWith("/files/")) return serveImage(res, url.pathname.slice("/files/".length));
	return serveStatic(res, url.pathname);
}

// A browser always sends Origin on cross-origin writes; it must match the Host it addressed, so
// an SSH port forward (any local port) still works. Clients without Origin (curl, agents) pass.
function isSameOrigin(req) {
	const origin = req.headers.origin;
	if (origin === undefined) return true;
	return origin.toLowerCase() === `http://${String(req.headers.host).toLowerCase()}`;
}

async function routeApi(req, res, method, url) {
	if (url.pathname === "/api/docs" || url.pathname.startsWith("/api/docs/")) {
		if (method !== "GET" && method !== "HEAD") throw new HttpError(405, "Method not allowed.");
		const rest = url.pathname.slice("/api/docs".length).replace(/^\/+$/, "");
		return rest ? sendDoc(res, rest.slice(1)) : sendJson(res, 200, await listDocs());
	}
	const segments = url.pathname
		.slice("/api/".length)
		.split("/")
		.filter(Boolean)
		.map(decodeSegment);
	const [resource, partId, itemId] = segments;
	const isRead = method === "GET" || method === "HEAD";
	if (resource === "answered" && segments.length === 1) {
		if (!isRead) throw new HttpError(405, "Method not allowed.");
		return sendJson(res, 200, await listAnswered());
	}
	if (resource === "proposal" && segments.length === 3) {
		if (!isRead) throw new HttpError(405, "Method not allowed.");
		return sendJson(res, 200, { proposed: await proposalWhenAnswered(partId, itemId) });
	}
	if (resource === "parts" && segments.length <= 2) {
		if (!isRead) throw new HttpError(405, "Method not allowed.");
		return sendJson(res, 200, partId === undefined ? await listParts(url.searchParams.has("open")) : await getPart(partId));
	}
	if ((resource === "index" || resource === "repo-paths") && segments.length === 1) {
		if (!isRead) throw new HttpError(405, "Method not allowed.");
		return sendJson(res, 200, resource === "index" ? await itemIndex() : await repoPaths());
	}
	if (resource === "items" && segments.length === 1) {
		if (!isRead) throw new HttpError(405, "Method not allowed.");
		return sendJson(res, 200, await listItems(url.searchParams.get("importance")));
	}
	if (resource === "export.md" && segments.length === 1) {
		if (!isRead) throw new HttpError(405, "Method not allowed.");
		return sendText(res, 200, await buildExport(url.searchParams.get("part")), {
			"Content-Disposition": 'inline; filename="richview-lockdown-answers.md"',
		});
	}
	if (resource === "redact" && segments.length === 3 && method === "POST") {
		return sendJson(res, 200, await redactAnswer(partId, itemId, await readJson(req)));
	}
	if (resource === "answers" && segments.length === 3) {
		if (method === "POST" && itemId === "bulk") return sendJson(res, 200, await saveBulk(partId, await readJson(req)));
		if (method === "PUT") return sendJson(res, 200, await saveOne(partId, itemId, await readJson(req)));
		if (method === "DELETE") return sendJson(res, 200, await clearOne(partId, itemId));
		throw new HttpError(405, "Method not allowed.");
	}
	throw new HttpError(404, "Not found.");
}

function decodeSegment(segment) {
	try {
		return decodeURIComponent(segment);
	} catch {
		throw new HttpError(400, "Malformed URL encoding.");
	}
}

async function readJson(req) {
	if (!/^application\/json\b/i.test(String(req.headers["content-type"] ?? ""))) {
		throw new HttpError(415, "Send the body as application/json.");
	}
	const chunks = [];
	let size = 0;
	for await (const chunk of req) {
		size += chunk.length;
		if (size > MAX_BODY_BYTES) throw new HttpError(413, "The request body is too large.");
		chunks.push(chunk);
	}
	try {
		return JSON.parse(Buffer.concat(chunks).toString("utf8"));
	} catch {
		throw new HttpError(400, "The request body is not valid JSON.");
	}
}

// ---------------------------------------------------------------------------------------------
// Part and answer files

async function partIds() {
	let names;
	try {
		names = await readdir(partsDir);
	} catch (error) {
		if (error.code === "ENOENT") return [];
		throw error;
	}
	return names
		.filter((name) => name.endsWith(".json"))
		.map((name) => name.slice(0, -".json".length))
		.filter((id) => ID_PATTERN.test(id));
}

const partFile = (id) => `docs/lockdown/parts/${id}.json`;
const answersPath = (id) => path.join(answersDir, `${id}.json`);
const digest = (text) => createHash("sha256").update(text).digest("hex").slice(0, 16);
const proposalHash = (item) => digest(item.proposed);
const isObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);

function orderFromId(id) {
	const match = /^(\d+)/.exec(id);
	return match ? Number(match[1]) : Number.MAX_SAFE_INTEGER;
}

async function loadPart(id) {
	let raw;
	try {
		raw = await readFile(path.join(partsDir, `${id}.json`), "utf8");
	} catch (error) {
		if (error.code === "ENOENT") return null;
		return brokenPart(id, null, `Cannot read the part file: ${error.message}`);
	}
	const revision = digest(raw);
	let data;
	try {
		data = JSON.parse(raw);
	} catch (error) {
		return brokenPart(id, revision, `Invalid JSON: ${error.message}`);
	}
	if (!isObject(data)) return brokenPart(id, revision, "The part file must contain a JSON object.");
	if (!Array.isArray(data.items)) return brokenPart(id, revision, "The part file has no items array.");
	return normalizePart(id, data, revision);
}

function brokenPart(id, revision, error) {
	return {
		id,
		file: partFile(id),
		revision,
		title: id,
		status: "unavailable",
		order: orderFromId(id),
		summary: "",
		items: [],
		warnings: [],
		error,
	};
}

const plain = (value) => (typeof value === "string" ? value.trim() : typeof value === "number" ? String(value) : "");
const list = (value) => (Array.isArray(value) ? value : value === undefined || value === null ? [] : [value]);

function markdown(value) {
	if (typeof value === "string") return value.trim();
	if (Array.isArray(value)) return value.map((line) => (typeof line === "string" ? line : JSON.stringify(line))).join("\n");
	if (value === undefined || value === null) return "";
	return typeof value === "object" ? JSON.stringify(value) : String(value);
}

function normalizePart(id, data, revision) {
	const warnings = [];
	if (data.id !== undefined && data.id !== id) {
		warnings.push(`The part's id "${plain(data.id)}" differs from its file name; the app uses "${id}".`);
	}
	const items = [];
	const seen = new Set();
	data.items.forEach((raw, index) => {
		const itemId = isObject(raw) ? plain(raw.id) : "";
		if (!itemId) {
			warnings.push(`Item ${index + 1} has no id and is not shown.`);
		} else if (seen.has(itemId)) {
			warnings.push(`Item id ${itemId} appears more than once; only the first is shown.`);
		} else {
			seen.add(itemId);
			items.push(normalizeItem(raw, itemId));
		}
	});
	return {
		id,
		file: partFile(id),
		revision,
		title: plain(data.title) || id,
		status: plain(data.status).toLowerCase() || "draft",
		order: Number.isFinite(data.order) ? data.order : orderFromId(id),
		summary: markdown(data.summary),
		items,
		warnings,
		error: null,
	};
}

function normalizeItem(raw, id) {
	return {
		...raw,
		id,
		kind: plain(raw.kind).toLowerCase() || "question",
		refId: plain(raw.refId),
		component: plain(raw.component),
		group: plain(raw.group),
		importance: IMPORTANCE.includes(plain(raw.importance).toLowerCase()) ? plain(raw.importance).toLowerCase() : "",
		title: plain(raw.title) || id,
		question: markdown(raw.question),
		why: markdown(raw.why),
		proposed: markdown(raw.proposed),
		options: list(raw.options)
			.map((option) =>
				isObject(option) ? { label: plain(option.label), detail: markdown(option.detail) } : { label: plain(option), detail: "" },
			)
			.filter((option) => option.label || option.detail),
		recommendation: markdown(raw.recommendation),
		confidence: plain(raw.confidence).toLowerCase() || "unknown",
		sources: list(raw.sources)
			.map((source) =>
				isObject(source)
					? { label: plain(source.label), ref: plain(source.ref), date: plain(source.date) }
					: { label: "", ref: plain(source), date: "" },
			)
			.filter((source) => source.label || source.ref),
		conflicts: markdown(raw.conflicts),
		images: list(raw.images).map(plain).filter(Boolean),
	};
}

async function loadAnswers(partId) {
	const file = answersPath(partId);
	let raw;
	try {
		raw = await readFile(file, "utf8");
	} catch (error) {
		if (error.code === "ENOENT") return { answers: {}, error: null };
		return { answers: {}, error: `Cannot read ${path.relative(root, file)}: ${error.message}.` };
	}
	try {
		const data = JSON.parse(raw);
		if (!isObject(data) || (data.answers !== undefined && !isObject(data.answers))) {
			throw new Error('expected {"partId": ..., "answers": {...}}');
		}
		return { answers: data.answers ?? {}, error: null };
	} catch (error) {
		return { answers: {}, error: `${path.relative(root, file)} is not valid: ${error.message}.` };
	}
}

function presentAnswer(answer, item) {
	if (!isObject(answer) || !DECISIONS.has(answer.decision)) return null;
	const stale = typeof answer.proposedHash === "string" && answer.proposedHash !== proposalHash(item);
	return { ...answer, stale };
}

function mergeAnswers(part, { answers, error }) {
	const known = new Set(part.items.map((item) => item.id));
	const items = part.items.map((item) => ({
		...item,
		answer: Object.hasOwn(answers, item.id) ? presentAnswer(answers[item.id], item) : null,
	}));
	const orphans = part.error
		? []
		: Object.entries(answers)
				.filter(([itemId, answer]) => !known.has(itemId) && isObject(answer))
				.map(([itemId, answer]) => ({ itemId, ...answer }));
	const decisions = { accept: 0, edit: 0, reject: 0, defer: 0 };
	let answeredCount = 0;
	let staleCount = 0;
	for (const item of items) {
		if (!item.answer) continue;
		answeredCount += 1;
		decisions[item.answer.decision] += 1;
		if (item.answer.stale) staleCount += 1;
	}
	return {
		...part,
		items,
		orphans,
		answersError: error,
		answersOnFile: Object.keys(answers).length,
		itemCount: items.length,
		answeredCount,
		decisions,
		staleCount,
	};
}

const byPartOrder = (a, b) => a.order - b.order || a.id.localeCompare(b.id);

async function loadMergedParts(onlyId) {
	const ids = (await partIds()).filter((id) => !onlyId || id === onlyId);
	const parts = await Promise.all(
		ids.map(async (id) => {
			const part = await loadPart(id);
			return part && mergeAnswers(part, await loadAnswers(id));
		}),
	);
	return parts.filter(Boolean).sort(byPartOrder);
}

// {itemId: partId} for every item of every readable part, read fresh, so the UI can link item IDs
// to their cards. An ID that appears in two parts (listParts warns) maps to the earlier part.
async function itemIndex() {
	const parts = (await Promise.all((await partIds()).map(loadPart))).filter(Boolean).sort(byPartOrder);
	const index = new Map();
	for (const part of parts) {
		for (const item of part.items) if (!index.has(item.id)) index.set(item.id, part.id);
	}
	return Object.fromEntries(index);
}

async function listParts(includeOpenIds) {
	const parts = await loadMergedParts();
	const owners = new Map();
	for (const part of parts) {
		for (const item of part.items) owners.set(item.id, [...(owners.get(item.id) ?? []), part.id]);
	}
	const summaries = parts.map((part) => {
		const warnings = [...part.warnings];
		const importance = Object.fromEntries(IMPORTANCE_LEVELS.map((level) => [level, { itemCount: 0, answeredCount: 0 }]));
		for (const item of part.items) {
			const counts = importance[item.importance || "unrated"];
			counts.itemCount += 1;
			if (item.answer) counts.answeredCount += 1;
		}
		for (const item of part.items) {
			const others = owners.get(item.id).filter((id) => id !== part.id);
			if (others.length) warnings.push(`Item id ${item.id} also appears in ${others.join(", ")}.`);
		}
		return {
			id: part.id,
			file: part.file,
			title: part.title,
			status: part.status,
			order: part.order,
			revision: part.revision,
			itemCount: part.itemCount,
			answeredCount: part.answeredCount,
			decisions: part.decisions,
			staleCount: part.staleCount,
			importance,
			orphanCount: part.orphans.length,
			...(includeOpenIds ? { openIds: part.items.filter(item => !item.answer).map(item => item.id) } : {}),
			warnings,
			error: part.error,
			answersError: part.answersError,
		};
	});
	const totals = {
		parts: summaries.length,
		ready: 0,
		itemCount: 0,
		answeredCount: 0,
		decisions: { accept: 0, edit: 0, reject: 0, defer: 0 },
		importance: Object.fromEntries(IMPORTANCE_LEVELS.map((level) => [level, { itemCount: 0, answeredCount: 0 }])),
	};
	for (const part of summaries) {
		if (part.status === "ready" && !part.error) totals.ready += 1;
		totals.itemCount += part.itemCount;
		totals.answeredCount += part.answeredCount;
		for (const decision of DECISIONS) totals.decisions[decision] += part.decisions[decision];
		for (const level of IMPORTANCE_LEVELS) {
			totals.importance[level].itemCount += part.importance[level].itemCount;
			totals.importance[level].answeredCount += part.importance[level].answeredCount;
		}
	}
	return { parts: summaries, totals };
}

// Items of every readable part with their answers and partId, in part order and then file order;
// `importance` (comma-separated levels, "unrated" included) limits them. `parts` lists every part
// with its revision, so a client can tell when any of them changed.
async function listItems(importance) {
	let wanted = null;
	if (importance !== null) {
		wanted = new Set(
			importance
				.split(",")
				.map((level) => level.trim().toLowerCase())
				.filter(Boolean),
		);
		for (const level of wanted) {
			if (!IMPORTANCE_LEVELS.includes(level)) throw new HttpError(400, `Unknown importance ${level}; use ${IMPORTANCE_LEVELS.join(", ")}.`);
		}
	}
	const parts = await loadMergedParts();
	return {
		parts: parts.map((part) => ({ id: part.id, title: part.title, status: part.status, order: part.order, revision: part.revision, error: part.error })),
		items: parts.flatMap((part) =>
			part.items.filter((item) => !wanted || wanted.has(item.importance || "unrated")).map((item) => ({ ...item, partId: part.id })),
		),
	};
}

// Decisions and wording come from the current files. Only the date comes from the log,
// which may contain superseded or redacted text.
async function listAnswered() {
	const [parts, log] = await Promise.all([loadMergedParts(), readFile(logFile, "utf8").catch(error => {
		if (error.code === "ENOENT") return "";
		throw error;
	})]);
	const dates = new Map();
	for (const line of log.split("\n")) {
		if (!line.trim()) continue;
		const { partId, itemId, action, at } = JSON.parse(line);
		const key = JSON.stringify([partId, itemId]);
		if (action === "clear") dates.delete(key);
		else if (["set", "redact"].includes(action) && Number.isFinite(Date.parse(at))) dates.set(key, at);
	}
	const items = [];
	for (const part of parts) {
		const add = (id, title, proposed, answer) => {
			const at = dates.get(JSON.stringify([part.id, id])) ?? null;
			items.push({ id, title, proposed, partId: part.id, partTitle: part.title, answer, at });
		};
		for (const item of part.items) {
			if (item.answer) add(item.id, item.title, item.proposed, item.answer);
		}
		for (const { itemId, ...answer } of part.orphans) add(itemId, null, null, answer);
	}
	items.sort((a, b) => (Date.parse(b.at) || 0) - (Date.parse(a.at) || 0));
	return { items };
}

const gitRead = promisify(execFile);
const proposalCache = new Map();

async function proposalWhenAnswered(partId, itemId) {
	const part = await getPart(partId);
	const item = part.items.find(candidate => candidate.id === itemId);
	if (!item?.answer) throw new HttpError(404, "Answered card not found.");
	if (!item.answer.stale) return item.proposed;
	const key = JSON.stringify([partId, itemId, item.answer.proposedHash]);
	if (!proposalCache.has(key)) {
		proposalCache.set(key, findProposal(partFile(partId), itemId, item.answer.proposedHash));
	}
	return proposalCache.get(key);
}

async function findProposal(file, itemId, expectedHash) {
	const git = async (...args) => (await gitRead("git", ["--git-dir", path.join(root, ".git"), ...args],
		{ timeout: 3000, maxBuffer: MAX_DOC_BYTES })).stdout;
	try {
		const revisions = await git("log", "-64", "--format=%H", "--", file);
		for (const revision of revisions.trim().split("\n").filter(Boolean)) {
			const snapshot = await git("show", `${revision}:${file}`).catch(() => "");
			if (!snapshot) continue;
			let data;
			try { data = JSON.parse(snapshot); } catch { continue; }
			const item = data.items?.find(candidate => candidate?.id === itemId);
			if (item && digest(markdown(item.proposed)) === expectedHash) return markdown(item.proposed);
		}
	} catch { /* Git or the repository history is unavailable. */ }
	return null;
}

async function getPart(id) {
	const part = ID_PATTERN.test(id) ? await loadPart(id) : null;
	if (!part) throw new HttpError(404, `Unknown part ${id}.`);
	return mergeAnswers(part, await loadAnswers(id));
}

// ---------------------------------------------------------------------------------------------
// Answer writes

function optionalString(value, name) {
	if (value === undefined || value === null) return "";
	if (typeof value !== "string") throw new HttpError(400, `${name} must be a string.`);
	if (value.length > MAX_FIELD_CHARS) throw new HttpError(400, `${name} is too long.`);
	return value;
}

function validateAnswer(body) {
	if (!isObject(body)) throw new HttpError(400, "Send a JSON object with decision, text and note.");
	if (!DECISIONS.has(body.decision)) {
		throw new HttpError(400, `decision must be accept, edit, reject or defer (got ${JSON.stringify(body.decision) ?? "nothing"}).`);
	}
	const text = optionalString(body.text, "text").trimEnd();
	const note = optionalString(body.note, "note").trim();
	if (body.decision === "edit" && !text.trim()) throw new HttpError(400, "An edit needs non-empty text.");
	return { decision: body.decision, text: body.decision === "edit" ? text : "", note };
}

async function saveOne(partId, itemId, body) {
	const answers = await applyChanges(partId, [{ itemId, ...validateAnswer(body) }], false);
	return { ok: true, partId, itemId, answer: answers[itemId] };
}

async function saveBulk(partId, body) {
	if (!Array.isArray(body) || body.length === 0) {
		throw new HttpError(400, "Send a non-empty JSON array of {itemId, decision, text, note}.");
	}
	if (body.length > MAX_BATCH) throw new HttpError(400, `A batch holds at most ${MAX_BATCH} answers.`);
	const errors = [];
	const seen = new Set();
	const changes = [];
	body.forEach((entry, index) => {
		try {
			const itemId = isObject(entry) ? entry.itemId : undefined;
			if (typeof itemId !== "string" || !itemId) throw new HttpError(400, "itemId is required.");
			if (seen.has(itemId)) throw new HttpError(400, `itemId ${itemId} appears twice.`);
			seen.add(itemId);
			changes.push({ itemId, ...validateAnswer(entry) });
		} catch (error) {
			errors.push(`Entry ${index}: ${error.message}`);
		}
	});
	if (errors.length) throw new HttpError(400, "Invalid batch; nothing was saved.", errors);
	return { ok: true, partId, answers: await applyChanges(partId, changes, true) };
}

async function clearOne(partId, itemId) {
	await applyChanges(partId, [{ itemId, clear: true }], false);
	return { ok: true, partId, itemId, answer: null };
}

async function applyChanges(partId, changes, bulk) {
	if (!ID_PATTERN.test(partId)) throw new HttpError(404, `Unknown part ${partId}.`);
	const file = answersPath(partId);
	return withLock(file, async () => {
		const part = await loadPart(partId);
		if (!part) throw new HttpError(404, `Unknown part ${partId}.`);
		if (part.error) {
			throw new HttpError(409, `Part ${partId} is unavailable (${part.error}). Try again once its file is complete.`);
		}
		const current = await loadAnswers(partId);
		if (current.error) throw new HttpError(409, `${current.error} Fix or move it before saving.`);
		const items = new Map(part.items.map((item) => [item.id, item]));
		const answers = new Map(Object.entries(current.answers));
		// Clearing also accepts an answer whose item has left the part, so orphans can be removed.
		const missing = changes
			.filter((change) => !items.has(change.itemId) && !(change.clear && answers.has(change.itemId)))
			.map((change) => change.itemId);
		if (missing.length) throw new HttpError(404, `Unknown item in part ${partId}: ${missing.join(", ")}.`);

		const at = new Date().toISOString();
		const result = new Map();
		const log = [];
		for (const change of changes) {
			const { itemId } = change;
			if (change.clear) {
				result.set(itemId, null);
				if (!answers.delete(itemId)) continue;
				log.push({ at, partId, itemId, decision: null, text: null, note: null, action: "clear" });
				continue;
			}
			const item = items.get(itemId);
			const record = { decision: change.decision };
			if (change.decision === "edit") record.text = change.text;
			if (change.note) record.note = change.note;
			record.proposedHash = proposalHash(item);
			record.updatedAt = at;
			answers.set(itemId, record);
			result.set(itemId, presentAnswer(record, item));
			log.push({
				at,
				partId,
				itemId,
				decision: record.decision,
				text: record.text ?? null,
				note: record.note ?? null,
				action: "set",
				...(bulk ? { bulk: true } : {}),
			});
		}
		if (!log.length) return Object.fromEntries(result);
		await mkdir(answersDir, { recursive: true });
		await writeJsonAtomic(file, { partId, answers: orderAnswers(answers, part.items) });
		await withLock(logFile, () => appendFile(logFile, `${log.map((entry) => JSON.stringify(entry)).join("\n")}\n`));
		return Object.fromEntries(result);
	});
}

// Redact the log first. If the answer rename fails, retrying is safe: no old text is restored.
// As with all writes, run only one server for this checkout; the locks are process-local.
async function redactAnswer(partId, itemId, body) {
	if (!isObject(body) || body.confirm !== "redact") throw new HttpError(400, 'Send confirm: "redact" and the approved replacement text.');
	const { text } = validateAnswer({ decision: "edit", text: body.text });
	if (!ID_PATTERN.test(partId)) throw new HttpError(404, `Unknown part ${partId}.`);
	const file = answersPath(partId);
	return withLock(file, () => withLock(logFile, async () => {
		const part = await loadPart(partId);
		if (!part || part.error) throw new HttpError(409, "Part is unavailable.");
		const item = part.items.find((entry) => entry.id === itemId);
		const current = await loadAnswers(partId);
		if (!item || current.error || !current.answers[itemId]) throw new HttpError(409, "Existing answer is unavailable.");
		const originalLog = await readFile(logFile, "utf8");
		const lines = originalLog.split("\n");
		if (lines.at(-1) === "") lines.pop();
		let entries;
		try {
			entries = lines.map((line) => {
				const entry = JSON.parse(line);
				if (!isObject(entry) || typeof entry.partId !== "string" || typeof entry.itemId !== "string") throw new Error("invalid log entry");
				return entry;
			});
		} catch {
			throw new HttpError(409, "Malformed log; nothing was changed.");
		}
		const at = new Date().toISOString();
		const answers = new Map(Object.entries(current.answers));
		answers.set(itemId, { decision: "edit", text, proposedHash: proposalHash(item), updatedAt: at });
		const sanitized = entries.map((entry, index) => entry.partId === partId && entry.itemId === itemId
			? JSON.stringify({ ...entry, text: "[redacted]", note: "[redacted]" }) : lines[index]);
		sanitized.push(JSON.stringify({ at, partId, itemId, action: "redact" }));
		let logTemp, answerTemp;
		try {
			logTemp = await stageText(logFile, sanitized.join("\n") + "\n");
			answerTemp = await stageText(file, JSON.stringify({ partId, answers: orderAnswers(answers, part.items) }, null, 2) + "\n");
			await rename(logTemp, logFile);
			await rename(answerTemp, file);
		} finally {
			if (logTemp) await rm(logTemp, { force: true });
			if (answerTemp) await rm(answerTemp, { force: true });
		}
		return { ok: true, partId, itemId };
	}));
}

async function stageText(file, text) {
	const temp = path.join(path.dirname(file), `.${path.basename(file)}.${process.pid}.${randomBytes(4).toString("hex")}.tmp`);
	const handle = await open(temp, "wx", 0o600);
	try {
		try { await handle.writeFile(text); await handle.sync(); } finally { await handle.close(); }
		return temp;
	} catch (error) {
		await rm(temp, { force: true });
		throw error;
	}
}

// Answers follow the part's item order so the file diffs cleanly; orphans go last.
function orderAnswers(answers, items) {
	const known = items.filter((item) => answers.has(item.id)).map((item) => item.id);
	const orphans = [...answers.keys()].filter((key) => !items.some((item) => item.id === key)).sort();
	return Object.fromEntries([...known, ...orphans].map((key) => [key, answers.get(key)]));
}

async function writeJsonAtomic(file, data) {
	const temp = path.join(path.dirname(file), `.${path.basename(file)}.${process.pid}.${randomBytes(4).toString("hex")}.tmp`);
	const handle = await open(temp, "w");
	try {
		await handle.writeFile(`${JSON.stringify(data, null, 2)}\n`);
		await handle.sync();
	} finally {
		await handle.close();
	}
	try {
		await rename(temp, file);
	} catch (error) {
		await rm(temp, { force: true });
		throw error;
	}
}

// ---------------------------------------------------------------------------------------------
// Markdown export for agents

const FINAL_LABEL = {
	accept: "Accepted text",
	edit: "Edited text",
	reject: "Rejected proposal",
	defer: "Deferred proposal",
	open: "Open proposal",
};

const oneLine = (text) => text.replace(/\s+/g, " ").trim();
const quote = (text) =>
	text
		.split("\n")
		.map((line) => (line.trim() ? `> ${line}` : ">"))
		.join("\n");

function countLine(counts) {
	return `${counts.answeredCount} of ${counts.itemCount} answered: ${counts.decisions.accept} accepted, ${counts.decisions.edit} edited, ${counts.decisions.reject} rejected, ${counts.decisions.defer} deferred, ${counts.itemCount - counts.answeredCount} open`;
}

// Titles of the component cards (the C- entities of part 03) by entity ID.
function componentTitles(parts) {
	const titles = new Map();
	for (const part of parts) {
		for (const item of part.items) {
			if (item.kind === "component" && item.refId && !titles.has(item.refId)) titles.set(item.refId, oneLine(item.title));
		}
	}
	return titles;
}

async function buildExport(onlyId) {
	// Every part is read so a one-part export still finds its component titles.
	const all = await loadMergedParts();
	const parts = onlyId ? all.filter((part) => part.id === onlyId) : all;
	if (onlyId && !parts.length) throw new HttpError(404, `Unknown part ${onlyId}.`);
	const components = componentTitles(all);
	const totals = { itemCount: 0, answeredCount: 0, decisions: { accept: 0, edit: 0, reject: 0, defer: 0 } };
	for (const part of parts) {
		totals.itemCount += part.itemCount;
		totals.answeredCount += part.answeredCount;
		for (const decision of DECISIONS) totals.decisions[decision] += part.decisions[decision];
	}
	const out = [
		"# RichView lockdown answers",
		"",
		`Exported ${new Date().toISOString()} from docs/lockdown. ${parts.length} part${parts.length === 1 ? "" : "s"}; ${countLine(totals)}.`,
		"",
		"Decisions: `accept` keeps the proposal as written; `edit` replaces it with Daniel's text; `reject` drops it from the rewrite; `defer` and `open` leave the item unresolved. `stale` marks a decision taken on an earlier version of the proposal; treat it as unresolved until Daniel confirms it again.",
		"",
	];
	for (const part of parts) {
		out.push(`## ${part.id}: ${oneLine(part.title)}`, "");
		if (part.error) {
			out.push(`Unavailable: ${part.error} Answers on file: ${part.answersOnFile}.`, "");
			continue;
		}
		out.push(`Status: ${part.status}. ${countLine(part)}.`, "");
		if (part.answersError) out.push(`Warning: ${part.answersError}`, "");
		for (const item of part.items) out.push(...exportItem(item, components));
		if (part.orphans.length) {
			out.push("### Answers whose item is no longer in this part", "");
			for (const orphan of part.orphans) {
				const note = typeof orphan.note === "string" && orphan.note ? `; note: ${oneLine(orphan.note)}` : "";
				out.push(`- ${orphan.itemId}: ${orphan.decision ?? "unknown"}${note}`);
			}
			out.push("");
		}
	}
	return `${out.join("\n").trimEnd()}\n`;
}

function exportItem(item, components) {
	const answer = item.answer;
	const decision = answer ? answer.decision : "open";
	const lines = [`### ${item.id}: ${oneLine(item.title)}`, ""];
	lines.push(`- Decision: ${decision}${answer?.stale ? " (stale: the proposal changed after this decision)" : ""}`);
	if (item.refId) lines.push(`- Entity: ${item.refId}`);
	if (item.component) {
		lines.push(`- Component: ${item.component} ${components.get(item.component) ?? "(no component card has this ID)"}`);
	}
	lines.push(
		`- Kind: ${item.kind}; confidence: ${item.confidence}${item.importance ? `; importance: ${item.importance}` : ""}${item.group ? `; group: ${oneLine(item.group)}` : ""}`,
	);
	if (answer?.updatedAt) lines.push(`- Answered: ${answer.updatedAt}`);
	lines.push("");
	const text = decision === "edit" ? answer.text : item.proposed;
	lines.push(`${FINAL_LABEL[decision]}:`, "", quote(text || "(no proposal)"), "");
	if (answer?.note) lines.push("Note:", "", quote(answer.note), "");
	return lines;
}

// ---------------------------------------------------------------------------------------------
// Files

// Resolves a relative path inside base, following symlinks, and returns the real path of a
// regular file that stays inside base; anything else is null.
async function safeResolve(base, relative) {
	if (!relative || relative.includes("\0") || relative.includes("\\")) return null;
	const normalized = path.posix.normalize(relative);
	if (normalized === ".." || normalized.startsWith("../") || path.posix.isAbsolute(normalized)) return null;
	const [baseReal, fileReal] = await Promise.all([
		realpath(base).catch(() => null),
		realpath(path.resolve(base, normalized)).catch(() => null),
	]);
	if (!baseReal || !fileReal || !fileReal.startsWith(baseReal + path.sep)) return null;
	const info = await stat(fileReal).catch(() => null);
	return info?.isFile() ? fileReal : null;
}

// ---------------------------------------------------------------------------------------------
// Guide documents: read-only Markdown from LOCKDOWN_DOCS and docs/guide/**.

// A repo path such as docs/guide/components/charts.md; no empty, dot or hidden segment.
function isDocPath(relative) {
	if (LOCKDOWN_DOCS.includes(relative)) return true;
	if (![GUIDE_DIR, TECH_DIR].some(dir => relative.startsWith(`${dir}/`)) || !relative.endsWith(".md")) return false;
	const segments = relative.split("/");
	return (
		segments.length <= 3 + MAX_GUIDE_DEPTH &&
		segments.every((segment) => segment && !segment.startsWith(".") && !/[\\\0]/.test(segment))
	);
}

// Returns the real file and its stat for an allowed document, or null. A lockdown file must be a
// regular file at exactly its own path; a guide file may be a symlink only to another file inside
// docs/guide, and docs/guide itself must not be a symlink.
async function resolveDoc(relative) {
	if (!isDocPath(relative)) return null;
	const rootReal = await realpath(root).catch(() => null);
	if (!rootReal) return null;
	const expected = path.join(rootReal, ...relative.split("/"));
	const real = await realpath(expected).catch(() => null);
	if (!real) return null;
	if (LOCKDOWN_DOCS.includes(relative)) {
		if (real !== expected) return null;
	} else {
		const guideExpected = path.join(rootReal, ...relative.split("/").slice(0, 2));
		const guideReal = await realpath(guideExpected).catch(() => null);
		if (guideReal !== guideExpected || !real.startsWith(guideExpected + path.sep)) return null;
	}
	const info = await stat(real).catch(() => null);
	return info?.isFile() ? { file: real, info } : null;
}

async function guideDocPaths(base) {
	const found = [];
	async function walk(dir, depth) {
		let entries;
		try {
			entries = await readdir(path.join(root, ...dir.split("/")), { withFileTypes: true });
		} catch {
			return;
		}
		for (const entry of entries) {
			if (found.length >= MAX_GUIDE_DOCS) return;
			if (entry.name.startsWith(".")) continue;
			const relative = `${dir}/${entry.name}`;
			// Dirent reports a symlink as a symlink, so linked directories are never walked.
			if (entry.isDirectory()) {
				if (depth < MAX_GUIDE_DEPTH) await walk(relative, depth + 1);
			} else if (entry.name.endsWith(".md") && (entry.isFile() || entry.isSymbolicLink())) {
				found.push(relative);
			}
		}
	}
	await walk(base, 0);
	return found;
}

// Titles are cached per file version, so polling the list rereads only changed files.
const docCache = new Map();

async function describeDoc(relative) {
	const resolved = await resolveDoc(relative);
	if (!resolved || resolved.info.size > MAX_DOC_BYTES) return null;
	const { file, info } = resolved;
	const version = `${info.size}:${info.mtimeMs}`;
	let cached = docCache.get(relative);
	if (cached?.file !== file || cached.version !== version) {
		const text = await readFile(file, "utf8").catch(() => null);
		if (text === null) return null;
		cached = { file, version, title: docTitle(text) || path.posix.basename(relative), revision: digest(text) };
		docCache.set(relative, cached);
	}
	return {
		path: relative,
		section: relative.split("/")[1],
		title: cached.title,
		revision: cached.revision,
		size: info.size,
		updatedAt: info.mtime.toISOString(),
	};
}

// The first ATX heading outside front matter and fenced code, as plain text.
function docTitle(text) {
	const lines = text.replace(/^\uFEFF/, "").split(/\r\n?|\n/);
	let start = 0;
	if (/^---\s*$/.test(lines[0] ?? "") && /^[\w-]+\s*:/.test(lines[1] ?? "")) {
		const end = lines.findIndex((line, index) => index > 0 && /^(?:---|\.\.\.)\s*$/.test(line));
		if (end > 0) start = end + 1;
	}
	let fence = null;
	for (const line of lines.slice(start)) {
		const marker = /^\s{0,3}(`{3,}|~{3,})/.exec(line)?.[1];
		if (fence) {
			if (marker && marker[0] === fence[0] && marker.length >= fence.length && !line.trim().slice(marker.length).trim()) fence = null;
			continue;
		}
		if (marker) {
			fence = marker;
			continue;
		}
		const heading = /^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$/.exec(line);
		if (heading) return plainHeading(heading[1]);
	}
	return "";
}

function plainHeading(text) {
	return text
		.replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
		.replace(/`+/g, "")
		.replace(/(\*\*|__|~~)(.+?)\1/g, "$2")
		.replace(/(^|[^\w*])\*([^*\s][^*]*?)\*(?!\w)/g, "$1$2")
		.replace(/\s+/g, " ")
		.trim()
		.slice(0, 200);
}

function compareDocPaths(a, b) {
	const left = a.split("/");
	const right = b.split("/");
	for (let index = 0; index < Math.min(left.length, right.length); index += 1) {
		if (left[index] === right[index]) continue;
		// README or index comes first in its folder.
		const rank = (segment, segments) =>
			index === segments.length - 1 && /^(?:readme|index)\.md$/i.test(segment) ? 0 : 1;
		return (
			rank(left[index], left) - rank(right[index], right) ||
			left[index].localeCompare(right[index], "en", { numeric: true, sensitivity: "base" }) ||
			left[index].localeCompare(right[index])
		);
	}
	return left.length - right.length;
}

async function listDocs() {
	const [guide, technical] = await Promise.all([guideDocPaths(GUIDE_DIR), guideDocPaths(TECH_DIR)]);
	const described = await Promise.all([...guide.sort(compareDocPaths), ...LOCKDOWN_DOCS, ...technical.sort(compareDocPaths)].map(describeDoc));
	return { docs: described.filter(Boolean) };
}

// ---------------------------------------------------------------------------------------------
// Repo paths the UI may link to GitHub: files of the evidence commit, and files on disk under the
// folders that live only on the lockdown branch.

async function repoPaths() {
	const [paths, lockdownPaths] = await Promise.all([pinnedRepoPaths(), lockdownRepoPaths()]);
	return { commit: PINNED_COMMIT, paths, lockdownPaths };
}

// The commit never changes, so git runs once. -z keeps unusual names unquoted and --full-tree lists
// the whole repository even when the root is a folder inside it. When git fails (not installed,
// not a repository, commit missing) the list is empty and the UI recognises repo paths as before.
let pinnedPaths;
function pinnedRepoPaths() {
	pinnedPaths ??= new Promise((resolve) => {
		execFile(
			"git",
			["-C", root, "ls-tree", "-r", "-z", "--full-tree", "--name-only", PINNED_COMMIT],
			{ encoding: "utf8", maxBuffer: 64 * 1024 * 1024, timeout: 20_000, windowsHide: true },
			(error, stdout) => {
				if (error) {
					console.error(`Cannot list ${PINNED_COMMIT} with git: ${oneLine(String(error.message))} Repo paths link as before.`);
					resolve([]);
					return;
				}
				resolve(stdout.split("\0").filter(Boolean));
			},
		);
	});
	return pinnedPaths;
}

// Listed fresh, since agents add parts, guide pages and screenshots while the app runs. Hidden
// entries are skipped, linked folders are not walked, and a lockdown folder that is itself a
// symlink (or sits under one) lists nothing, as for the Guide view.
async function lockdownRepoPaths() {
	const rootReal = await realpath(root).catch(() => null);
	if (!rootReal) return [];
	const found = [];
	async function walk(dir, relative, depth) {
		let entries;
		try {
			entries = await readdir(dir, { withFileTypes: true });
		} catch {
			return;
		}
		for (const entry of entries) {
			if (found.length >= MAX_LOCKDOWN_PATHS) return;
			if (entry.name.startsWith(".")) continue;
			const child = `${relative}/${entry.name}`;
			if (entry.isDirectory()) {
				if (depth < MAX_LOCKDOWN_DEPTH) await walk(path.join(dir, entry.name), child, depth + 1);
			} else if (entry.isFile() || entry.isSymbolicLink()) {
				found.push(child);
			}
		}
	}
	for (const tree of LOCKDOWN_TREES) {
		const expected = path.join(rootReal, ...tree.split("/"));
		if ((await realpath(expected).catch(() => null)) === expected) await walk(expected, tree, 0);
	}
	return found.sort();
}

async function sendDoc(res, encoded) {
	const segments = encoded.split("/").map(decodeSegment);
	if (segments.some((segment) => !segment || segment === "." || segment === ".." || /[/\\\0]/.test(segment))) {
		throw new HttpError(404, "Not found.");
	}
	const relative = segments.join("/");
	const resolved = await resolveDoc(relative);
	if (!resolved) throw new HttpError(404, `No document at ${relative}.`);
	if (resolved.info.size > MAX_DOC_BYTES) throw new HttpError(413, "The document is too large to show.");
	const text = await readFile(resolved.file, "utf8");
	res.writeHead(200, {
		"Content-Type": "text/markdown; charset=utf-8",
		"Cache-Control": "no-store",
		"X-Doc-Revision": digest(text),
		...SECURITY_HEADERS,
	});
	res.end(text);
}

async function serveImage(res, encoded) {
	const relative = path.posix.normalize(decodeSegment(encoded));
	const dir = IMAGE_DIRS.find((candidate) => relative.startsWith(`${candidate}/`));
	const type = IMAGE_TYPES[path.posix.extname(relative).toLowerCase()];
	if (dir) {
		const expected = path.join(await realpath(root), dir);
		if (await realpath(expected).catch(() => null) !== expected) throw new HttpError(404, "Not found.");
	}
	const file = dir && type ? await safeResolve(path.join(root, dir), relative.slice(dir.length + 1)) : null;
	if (!file) throw new HttpError(404, "Not found.");
	res.writeHead(200, {
		"Content-Type": type,
		"Cache-Control": "no-cache",
		...SECURITY_HEADERS,
		...(type === "image/svg+xml" ? { "Content-Security-Policy": SVG_CSP } : {}),
	});
	createReadStream(file).pipe(res);
}

async function serveStatic(res, pathname) {
	const relative = pathname === "/" ? "index.html" : decodeSegment(pathname.slice(1));
	const file = await safeResolve(publicDir, relative);
	const type = file && STATIC_TYPES[path.extname(file).toLowerCase()];
	if (!type) throw new HttpError(404, "Not found.");
	res.writeHead(200, {
		"Content-Type": type,
		"Cache-Control": "no-cache",
		"Content-Security-Policy": PAGE_CSP,
		...SECURITY_HEADERS,
	});
	res.end(await readFile(file));
}

function sendJson(res, status, data) {
	res.writeHead(status, {
		"Content-Type": "application/json; charset=utf-8",
		"Cache-Control": "no-store",
		...SECURITY_HEADERS,
	});
	res.end(JSON.stringify(data));
}

function sendText(res, status, text, headers = {}) {
	res.writeHead(status, {
		"Content-Type": "text/plain; charset=utf-8",
		"Cache-Control": "no-store",
		...SECURITY_HEADERS,
		...headers,
	});
	res.end(text);
}
