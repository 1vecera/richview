// RichView lockdown review UI. Plain browser JavaScript, no dependencies; server.mjs persists answers.
"use strict";

const REPO = "https://github.com/1vecera/richview";
const PINNED_BLOB = `${REPO}/blob/b4db233/`;
const PINNED_TREE = `${REPO}/tree/b4db233/`;
// Lockdown files, the guide and this app are not in the pinned commit; they live on the lockdown branch.
const LOCKDOWN_BLOB = `${REPO}/blob/lockdown/richview-20261021/`;
const LOCKDOWN_TREE = `${REPO}/tree/lockdown/richview-20261021/`;
const LOCKDOWN_PATHS = /^(?:docs\/lockdown|docs\/guide|tools\/lockdown-app)(?:\/|$)/;
// Item IDs of every part (INT-01, FEA-RP-09, OPN-58); one links to its card when an item has it.
const ITEM_ID = /(?<![\w-])(?:INT|DEL|ARC|FEA-(?:CH|RP|SF|BR)|TYP|STY|PRF|OPN)-\d+(?!\w)/g;
// The cards to answer before any part: they decide which part holds where parts disagree.
const FIRST_GROUP = { partId: "08-open-work", group: "Cross-part contradictions", firstItem: "OPN-50", count: 19 };
// Choosing an option saves an edit whose first line names it.
const CHOICE_PREFIX = "Chosen option: ";
const OLD_REPO_URL = /^https:\/\/github\.com\/richview-universe\/richview-v3(?=[/?#]|$)/i;
const SAME_REPO = /^(?:richview-universe\/richview-v3|1vecera\/richview)$/i;
const LOCKDOWN_DOCS = new Set(
	["README", "GOALS", "RULES", "BOUNDARIES", "EVIDENCE", "DELIVERY", "ids"].map((name) => `docs/lockdown/${name}.md`),
);
// Top-level entries of the repository (b4db233 plus tools/), so Markdown can tell repo paths from prose.
const REPO_DIRS = /^(?:\.claude|\.github|assets|benchmarks|cloudflare|crates|docs|examples|packages|richview-chart|scripts|skill|tools)\//;
const REPO_FILES = new Set([
	".dockerignore",
	".editorconfig",
	".gitignore",
	".node-version",
	"CLAUDE.md",
	"Cargo.lock",
	"Cargo.toml",
	"Dockerfile",
	"LICENSE",
	"LICENSING.md",
	"README.md",
	"llms.txt",
	"package-lock.json",
	"package.json",
	"rust-toolchain.toml",
	"wrangler.interest-preview.toml",
	"wrangler.toml",
]);
const SERVED_IMAGE = /^(?:docs\/lockdown\/screenshots|docs\/guide|docs\/how-it-works\/diagrams)\/.+\.(?:avif|gif|jpe?g|png|svg|webp)$/i;
const DECISIONS = ["accept", "edit", "reject", "defer"];
const DECISION_BUTTON = { accept: "Accept", edit: "Edit", reject: "Reject", defer: "Defer" };
const DECISION_STATE = { accept: "Accepted", edit: "Edited", reject: "Rejected", defer: "Deferred" };
// What an answered card or row says it holds; a chosen option shows as "Chosen: <label>".
const DECISION_BADGE = { accept: "Accepted", edit: "Your edit", reject: "Rejected", defer: "Deferred" };
const CONFIDENCE_HELP = {
	observed: "Observed: verified in code or a running build",
	stated: "Stated: written in a doc, PR, ticket or note, not verified",
	inferred: "Inferred: the proposer's reading of several sources",
	unknown: "Unknown: no evidence, you decide",
};
const KEY_DECISIONS = { a: "accept", r: "reject", d: "defer" };
// Component filter value for items that name no component; component IDs start with C-.
const NO_COMPONENT = "none";
const POLL_MS = 5000;
const AUTOSAVE_MS = 700;
// Parts with more items than this open as rows unless Daniel chose otherwise for that part.
const ROWS_OVER = 40;
// An item's optional importance, most important first; an item without one is unrated and sorts last.
const IMPORTANCE = ["key", "important", "detail"];
const IMPORTANCE_LABEL = { key: "Key", important: "Important", detail: "Detail" };
const IMPORTANCE_HELP = {
	key: "Key: shapes everything else, is hard to undo or needs your go; answer these first",
	important: "Important: a real choice with a visible but local effect",
	detail: "Detail: follows from the key answers or is fine-grained",
};
// Importance filter values: every item, key items only, or key and important ones.
const IMPORTANCE_FILTERS = ["all", "key", "important"];
// The cross-part Key decisions view (#/key) reuses the part view under this ID, which no part
// file can have (part IDs start with a letter or digit).
const KEY_VIEW = ":key";
const KEY_HREF = "#/key";
const OPEN_VIEW = ":open";
const newFilter = (importance) => ({ view: "all", kind: "any", component: "any", topic: "any", query: "", importance });

// Unsaved changes survive failed saves, reloads and closed tabs in this browser (journal.js).
const J = globalThis.LockdownJournal;
const store = (() => {
	try {
		return window.localStorage;
	} catch {
		return null;
	}
})();

const $ = (selector) => document.querySelector(selector);
const els = {};
const state = {
	view: null,
	loaded: false,
	parts: [],
	totals: null,
	listError: "",
	docs: [],
	docsError: "",
	signature: "",
	part: null,
	partId: null,
	loadSeq: 0,
	loading: false,
	pending: Promise.resolve(),
	items: new Map(),
	ui: new Map(),
	// One set of filters for parts and one for the Key decisions view; filter is the active one.
	filters: {
		part: newFilter(prefIn("importance", IMPORTANCE_FILTERS, "all")),
		key: newFilter(prefIn("importance.key", ["key", "important"], "key")),
		open: newFilter("all"),
	},
	filter: null,
	collapse: readPref("collapse") === "1",
	compact: false,
	selection: new Set(),
	selectAnchor: null,
	selectionAsk: null,
	selectionBusy: false,
	selectionError: "",
	current: null,
	bulk: "idle",
	bulkError: "",
	doc: null,
	docSeq: 0,
	refreshTimer: 0,
	toastTimer: 0,
	// Link data, loaded before a part or document renders: item ID to part ID, and the repo paths
	// that exist (null until loaded, or when the server cannot list them).
	itemIndex: new Map(),
	repo: null,
	links: null,
	linksKey: "",
	firstGroup: null,
	firstGroupKey: "",
	// Key item progress for the rail and start page when the server does not count it (one started
	// before importance existed): computed from the items, read again when a part changes.
	keyStats: null,
	keyStatsKey: "",
};
state.filter = state.filters.part;

function init() {
	Object.assign(els, {
		main: $("#main"),
		overall: $("#overall"),
		unsaved: $("#unsaved"),
		rail: $("#rail"),
		railStart: $("#rail-start"),
		railKey: $("#rail-key"),
		railList: $("#rail-list"),
		railEmpty: $("#rail-empty"),
		guideDocs: $("#guide-docs"),
		guideEmpty: $("#guide-empty"),
		partsToggle: $("#parts-toggle"),
		mastheadGuide: $("#masthead-guide"),
		banner: $("#banner"),
		page: $("#page"),
		partView: $("#part-view"),
		head: $("#part-head"),
		toolbar: $("#toolbar"),
		query: $("#filter-query"),
		view: $("#filter-view"),
		kind: $("#filter-kind"),
		component: $("#filter-component"),
		componentField: $("#component-field"),
		importance: $("#filter-importance"),
		topic: $("#filter-topic"),
		topicField: $("#topic-field"),
		viewField: $("#view-field"),
		unanswered: $("#filter-unanswered"),
		collapse: $("#filter-collapse"),
		collapseField: $("#collapse-field"),
		layout: $("#layout"),
		showing: $("#showing"),
		listHead: $("#list-head"),
		selectAll: $("#select-all"),
		selectAllLabel: $("#select-all-label"),
		cards: $("#cards"),
		selectionBar: $("#selection-bar"),
		toast: $("#toast"),
		help: $("#help"),
		helpToggle: $("#help-toggle"),
		helpClose: $("#help-close"),
	});
	els.collapse.checked = state.collapse;
	els.query.addEventListener(
		"input",
		debounce(() => {
			state.filter.query = els.query.value.trim().toLowerCase();
			applyFilters();
		}, 120),
	);
	els.query.addEventListener("keydown", (event) => {
		if (event.key === "Escape") {
			els.query.blur();
			focusCurrent();
		}
	});
	els.view.addEventListener("change", () => {
		state.filter.view = els.view.value;
		els.unanswered.checked = state.filter.view === "unanswered";
		applyFilters();
	});
	els.kind.addEventListener("change", () => {
		state.filter.kind = els.kind.value;
		applyFilters();
	});
	els.component.addEventListener("change", () => {
		state.filter.component = els.component.value;
		applyFilters();
	});
	// The importance choice is remembered: one for every part, one for the Key decisions view.
	els.importance.addEventListener("change", () => {
		state.filter.importance = els.importance.value;
		if (state.partId !== OPEN_VIEW) writePref(isKeyView() ? "importance.key" : "importance", els.importance.value);
		applyFilters();
		renderPartProgress();
	});
	els.topic.addEventListener("change", () => {
		state.filter.topic = els.topic.value;
		applyFilters();
	});
	// "Unanswered only" is the quick form of Show: Unanswered, and the Key decisions view's only one.
	els.unanswered.addEventListener("change", () => {
		state.filter.view = els.unanswered.checked ? "unanswered" : "all";
		els.view.value = state.filter.view;
		applyFilters();
	});
	els.collapse.addEventListener("change", () => {
		state.collapse = els.collapse.checked;
		writePref("collapse", state.collapse ? "1" : "0");
		for (const ui of state.ui.values()) ui.expanded = false;
		for (const item of state.items.values()) updateCard(item);
	});
	els.layout.addEventListener("click", (event) => {
		const button = event.target.closest("[data-layout]");
		if (button) setCompact(button.dataset.layout === "rows");
	});
	els.selectAll.addEventListener("change", () => selectShown(els.selectAll.checked));
	els.partsToggle.addEventListener("click", () => toggleRail());
	els.helpToggle.addEventListener("click", () => toggleHelp());
	els.helpClose.addEventListener("click", () => toggleHelp(false));
	els.cards.addEventListener("click", onCardClick);
	els.cards.addEventListener("input", onCardInput);
	els.cards.addEventListener("keydown", onCardKeydown);
	els.cards.addEventListener("focusin", onCardFocus);
	els.cards.addEventListener("focusout", onCardBlur);
	els.head.addEventListener("click", onHeadClick);
	els.selectionBar.addEventListener("click", onSelectionClick);
	els.banner.addEventListener("click", (event) => {
		if (event.target.closest("[data-act='reload']")) reloadPart();
	});
	// Images that fail to load inside rendered Markdown become a visible "not found" line.
	document.addEventListener("error", onImageError, true);
	document.addEventListener("click", onItemLinkClick);
	document.addEventListener("keydown", onKey);
	window.addEventListener("hashchange", route);
	window.addEventListener("pagehide", flushAll);
	window.addEventListener("beforeunload", onBeforeUnload);
	window.addEventListener("online", () => retryJournal());
	window.addEventListener("storage", (event) => {
		if (event.key === J.KEY) renderUnsaved();
	});
	els.unsaved.addEventListener("click", onUnsavedClick);
	document.addEventListener("visibilitychange", () => {
		if (document.visibilityState === "visible") refresh();
		else flushAll();
	});
	setInterval(() => {
		if (document.visibilityState === "visible") refresh();
	}, POLL_MS);
	route();
	refresh();
}

// ---------------------------------------------------------------------------------------------
// Data

async function api(method, url, body) {
	const init = { method, headers: { Accept: "application/json" } };
	if (body !== undefined) {
		init.headers["Content-Type"] = "application/json";
		init.body = JSON.stringify(body);
		init.keepalive = init.body.length < 60_000;
	} else if (method !== "GET") {
		init.keepalive = true;
	}
	let response;
	try {
		response = await fetch(url, init);
	} catch {
		throw new Error("the app server is not reachable");
	}
	const data = await response.json().catch(() => ({}));
	if (!response.ok) {
		const error = new Error(data.error || `${response.status} ${response.statusText}`);
		error.status = response.status;
		throw error;
	}
	return data;
}

// Loads what links need before a part or document renders. A change to any part or document
// reloads it on the next poll; links already on screen update when their part or page renders again.
function linksReady() {
	state.links ??= loadLinks();
	return state.links;
}

async function loadLinks() {
	const [index, paths] = await Promise.allSettled([loadItemIndex(), api("GET", "/api/repo-paths")]);
	// A failed reload keeps what loaded before.
	if (index.status === "fulfilled") state.itemIndex = index.value;
	if (paths.status === "fulfilled") state.repo = repoSets(paths.value);
	// A server started before /api/repo-paths existed answers 404; paths then link as before.
	else if (paths.reason?.status === 404) state.repo = null;
}

async function loadItemIndex() {
	try {
		return new Map(Object.entries(await api("GET", "/api/index")));
	} catch (error) {
		if (error.status !== 404) throw error;
	}
	// A server started before /api/index existed: build the same index from the parts.
	const { parts } = await api("GET", "/api/parts");
	const loaded = await Promise.all(
		parts.filter((part) => !part.error).map((part) => api("GET", `/api/parts/${encodeURIComponent(part.id)}`).catch(() => null)),
	);
	const index = new Map();
	for (const part of loaded) for (const item of part?.items ?? []) if (!index.has(item.id)) index.set(item.id, part.id);
	return index;
}

// The paths that exist at the pinned commit (null when git could not list them) and under the
// lockdown folders, each with every folder above its files, since folders link too.
function repoSets(data) {
	const listed = (value) => (Array.isArray(value) ? value.filter((entry) => typeof entry === "string") : []);
	const pinned = listed(data?.paths);
	return { pinned: pinned.length ? withFolders(pinned) : null, lockdown: withFolders(listed(data?.lockdownPaths)) };
}

function withFolders(paths) {
	const folders = new Set();
	for (const file of paths) {
		for (let at = file.lastIndexOf("/"); at > 0; at = file.lastIndexOf("/", at - 1)) {
			const folder = file.slice(0, at);
			if (folders.has(folder)) break;
			folders.add(folder);
		}
	}
	return { files: new Set(paths), folders };
}

async function refresh() {
	const partsUrl = state.partId === OPEN_VIEW ? "/api/parts?open" : "/api/parts";
	const [parts, docs] = await Promise.allSettled([api("GET", partsUrl), api("GET", "/api/docs")]);
	if (parts.status === "fulfilled") {
		state.parts = parts.value.parts;
		state.totals = parts.value.totals;
		state.listError = "";
	} else {
		state.listError = parts.reason.message;
	}
	if (docs.status === "fulfilled") {
		state.docs = docs.value.docs;
		state.docsError = "";
	} else {
		state.docsError = docs.reason.message;
	}
	state.loaded = true;
	// A changed part file or document list can add item IDs and repo paths, so links reload.
	const linksKey = JSON.stringify([state.parts.map((part) => `${part.id}:${part.revision}`), state.docs.map((doc) => doc.path)]);
	if (linksKey !== state.linksKey) {
		if (state.linksKey && state.links) state.links = loadLinks();
		state.linksKey = linksKey;
	}
	const signature = JSON.stringify([state.parts, state.totals, state.listError, state.docs, state.docsError]);
	if (signature !== state.signature) {
		state.signature = signature;
		renderRail();
		renderOverall();
		if (state.view === "start") renderStart();
		if (state.view === "technical") {
			if (technicalPages().some(doc => doc.path === technicalIndex)) route();
			else showTechnicalIndex();
		}
	}
	renderUnsaved();
	if (parts.status === "fulfilled") {
		retryJournal();
		loadKeyStats();
	}
	if (state.view === "part") checkOpenPart();
	else if (state.view === "doc") checkOpenDoc();
	else if (state.view === "start") loadFirstGroup();
	else if (state.view === "answered") state.refreshAnswers?.();
}

// The cross-part contradiction cards for the start page: their first item and progress, read
// again when part 08 or its answers change.
async function loadFirstGroup() {
	const summary = state.parts.find((part) => part.id === FIRST_GROUP.partId);
	const key = summary && !summary.error ? `${summary.revision}:${summary.answeredCount}:${summary.staleCount}` : "";
	if (key === state.firstGroupKey) return;
	state.firstGroupKey = key;
	let info = null;
	if (key) {
		try {
			const part = await api("GET", `/api/parts/${encodeURIComponent(FIRST_GROUP.partId)}`);
			const items = byImportance(
				part.items.filter((item) => item.group === FIRST_GROUP.group),
				() => "",
			);
			if (items.length) info = { first: items[0].id, count: items.length, answered: items.filter((item) => item.answer).length };
		} catch {
			if (state.firstGroupKey === key) state.firstGroupKey = "";
			return;
		}
	}
	if (state.firstGroupKey !== key) return;
	state.firstGroup = info;
	if (state.view === "start") renderStart();
}

// Every key or important item of every part, each with its partId and answer: from /api/items, or
// from every part when the server predates it.
async function loadRatedItems() {
	try {
		const data = await api("GET", "/api/items?importance=key,important");
		return { parts: data.parts, items: data.items };
	} catch (error) {
		if (error.status !== 404) throw error;
	}
	const { parts } = await api("GET", "/api/parts");
	const loaded = await Promise.all(parts.filter((part) => !part.error).map((part) => api("GET", `/api/parts/${encodeURIComponent(part.id)}`)));
	const items = loaded.flatMap((part) => part.items.map((item) => ({ ...item, partId: part.id })));
	return { parts, items: items.filter((item) => importanceIn(item, "important")) };
}

// Key items and how many are answered, across every part; null until known.
function keyProgress() {
	const parts = state.parts.filter((part) => !part.error);
	if (parts.length && parts.every((part) => part.importance)) {
		const itemCount = parts.reduce((sum, part) => sum + part.importance.key.itemCount, 0);
		const answeredCount = parts.reduce((sum, part) => sum + part.importance.key.answeredCount, 0);
		return { itemCount, answeredCount };
	}
	return parts.length ? state.keyStats : { itemCount: 0, answeredCount: 0 };
}

// One part's key items and how many are answered; null until known.
function partKeyProgress(part) {
	if (part.importance) return part.importance.key;
	return state.keyStats ? (state.keyStats.byPart.get(part.id) ?? { itemCount: 0, answeredCount: 0 }) : null;
}

// "key 3 of 24" beside a part's progress; nothing when the part has no key items or it is not known.
const keyCount = (counts, className) =>
	counts?.itemCount ? el("span", { class: className, title: "Key items answered" }, `key ${counts.answeredCount} of ${counts.itemCount}`) : null;

// Counts key items itself when the server's part list does not (a server started before importance).
async function loadKeyStats() {
	const parts = state.parts.filter((part) => !part.error);
	if (!parts.length || parts.every((part) => part.importance)) return;
	const key = parts.map((part) => `${part.id}:${part.revision}:${part.answeredCount}`).join("|");
	if (key === state.keyStatsKey) return;
	state.keyStatsKey = key;
	let stats;
	try {
		const keyItems = (await loadRatedItems()).items.filter((item) => importanceOf(item) === "key");
		stats = { itemCount: keyItems.length, answeredCount: keyItems.filter((item) => item.answer).length, byPart: new Map() };
		for (const item of keyItems) {
			const counts = stats.byPart.get(item.partId) ?? { itemCount: 0, answeredCount: 0 };
			counts.itemCount += 1;
			if (item.answer) counts.answeredCount += 1;
			stats.byPart.set(item.partId, counts);
		}
	} catch {
		if (state.keyStatsKey === key) state.keyStatsKey = "";
		return;
	}
	if (state.keyStatsKey !== key) return;
	state.keyStats = stats;
	renderRail();
	if (state.view === "start") renderStart();
}

function scheduleRefresh() {
	clearTimeout(state.refreshTimer);
	state.refreshTimer = setTimeout(refresh, 400);
}

function checkOpenPart() {
	if (!state.partId) return;
	if (isKeyView() || state.partId === OPEN_VIEW) {
		checkCrossPartView();
		return;
	}
	const summary = state.parts.find((part) => part.id === state.partId);
	if (!state.part) {
		if (summary && !state.loading) openPart(state.partId);
		return;
	}
	if (!summary) {
		showBanner("This part's file is no longer in docs/lockdown/parts/. Answers already saved stay on disk.");
		return;
	}
	if (summary.revision === state.part.revision) {
		hideBanner();
		return;
	}
	if (state.part.error || state.part.items.length === 0) {
		reloadPart();
		return;
	}
	showBanner("This part changed on disk since you opened it. Your saved answers stay.", "Show the new version");
}

// The Key decisions view is built from every part: it offers the new version when any part file
// changes, and reloads by itself while it has no items.
function checkCrossPartView() {
	if (!state.part) {
		if (!state.loading) openPart(state.partId);
		return;
	}
	if (partsRevision(state.parts) === state.part.revision) {
		if (state.partId === OPEN_VIEW && state.parts.every(part => part.openIds)) {
			const open = new Set(state.parts.flatMap(part => part.openIds));
			const local = state.part.items.filter(item => !item.answer);
			if (local.length !== open.size || local.some(item => !open.has(item.id))) {
				const busy = state.selection.size || els.cards.contains(document.activeElement)
					|| [...state.ui.values()].some(ui => ui.editing || ui.noteOpen || ["saving", "error", "conflict"].includes(ui.status));
				if (!state.loading && !busy) reloadPart();
				else showBanner("The open questions changed since you opened this list.", "Show the new version");
				return;
			}
		}
		hideBanner();
		return;
	}
	if (state.part.items.length === 0) {
		reloadPart();
		return;
	}
	showBanner("A part changed on disk since you opened this view. Your saved answers stay.", "Show the new version");
}

const partsRevision = (parts) => parts.map((part) => `${part.id}:${part.revision}`).join("|");

// Routes: #/ is the start page, #/key[/<item>] the key decisions of every part, #/part/<part>[/<item>]
// a part, #/guide/<repo path>[#<heading>] a document.
function parseHash() {
	const hash = location.hash;
	try {
		let match = /^#\/part\/([^/#]+)(?:\/([^#]+))?$/.exec(hash);
		if (match) {
			return { view: "part", partId: decodeURIComponent(match[1]), itemId: match[2] ? decodeURIComponent(match[2]) : null };
		}
		match = /^#\/key(?:\/([^#]+))?$/.exec(hash);
		if (match) return { view: "part", partId: KEY_VIEW, itemId: match[1] ? decodeURIComponent(match[1]) : null };
		match = /^#\/open(?:\/([^#]+))?$/.exec(hash);
		if (match) return { view: "part", partId: OPEN_VIEW, itemId: match[1] ? decodeURIComponent(match[1]) : null };
		if (hash === "#/answered") return { view: "answered" };
		if (hash === "#/how-it-works") {
			return technicalPages().some(doc => doc.path === technicalIndex)
				? { view: "doc", path: technicalIndex } : { view: "technical" };
		}
		match = /^#\/guide\/([^#]+)(?:#(.*))?$/.exec(hash);
		if (match) {
			const docPath = match[1].split("/").map(decodeURIComponent).join("/");
			return { view: "doc", path: docPath, anchor: match[2] ? decodeURIComponent(match[2]) : null };
		}
	} catch {}
	return { view: "start" };
}

async function route() {
	const target = parseHash();
	state.refreshAnswers = null;
	toggleRail(false);
	if (target.view === "part") {
		leaveDoc();
		// Saves still running in the view being left land before the next one reads its answers.
		if (state.partId && state.partId !== target.partId) leavePart();
		showView("part");
		if (target.partId !== state.partId || !state.part) await openPart(target.partId);
		// A link or address that names an item shows it, even when filters, rows or collapse hid it.
		if (target.itemId && state.partId === target.partId && state.ui.has(target.itemId)) revealItem(target.itemId);
	} else if (target.view === "doc") {
		leavePart();
		showView("doc");
		renderRail();
		await openDoc(target.path, target.anchor);
	} else if (target.view === "answered" || target.view === "technical") {
		leavePart();
		leaveDoc();
		showView(target.view);
		window.scrollTo(0, 0);
		if (target.view === "answered") await openAnswered();
		else showTechnicalIndex();
	} else {
		leavePart();
		leaveDoc();
		showView("start");
		renderStart();
		window.scrollTo(0, 0);
		loadFirstGroup();
		if (!state.links) {
			linksReady().then(() => {
				if (state.view === "start") renderStart();
			});
		}
	}
	renderRail();
	revealInRail();
}

// Scrolls the rail, not the page, so the entry for what is open is in view.
function revealInRail() {
	if (getComputedStyle(els.rail).position !== "sticky") return;
	const link = els.rail.querySelector("[aria-current='page']");
	if (!link) return;
	const rail = els.rail.getBoundingClientRect();
	const box = link.getBoundingClientRect();
	const bottom = Math.min(rail.bottom, window.innerHeight);
	if (box.top < rail.top || box.bottom > bottom) els.rail.scrollTop += box.top - rail.top - 48;
}

function showView(view) {
	state.view = view;
	document.body.dataset.view = view;
	els.page.hidden = view === "part";
	els.partView.hidden = view !== "part";
	if (view !== "part") hideBanner();
	renderSelectionBar();
}

async function openPart(partId, { keep = false } = {}) {
	flushAll();
	const seq = ++state.loadSeq;
	const previous = keep ? { current: state.current, scrollY: window.scrollY } : null;
	state.partId = partId;
	state.loading = true;
	if (!keep) {
		state.part = null;
		state.current = null;
		state.bulk = "idle";
		renderLoading();
	}
	renderRail();
	// Saves still running from a part left a moment ago land before this part is read.
	await state.pending;
	await linksReady();
	try {
		const part = partId === KEY_VIEW ? await loadKeyView() : partId === OPEN_VIEW ? await loadOpenView() : await loadPart(partId);
		if (seq !== state.loadSeq) return;
		state.loading = false;
		state.part = part;
		state.filter = state.filters[part.virtual || "part"];
		state.items = new Map(part.items.map((item) => [item.id, item]));
		state.ui = new Map();
		state.selection.clear();
		state.selectAnchor = null;
		state.selectionAsk = null;
		state.selectionError = "";
		state.compact = compactPref(part);
		renderPart();
		restoreJournal();
		hideBanner();
		if (previous) {
			window.scrollTo(0, previous.scrollY);
			if (previous.current && state.ui.has(previous.current)) setCurrent(previous.current);
		} else {
			window.scrollTo(0, 0);
		}
	} catch (error) {
		if (seq !== state.loadSeq) return;
		state.part = null;
		state.items = new Map();
		renderPartError(error.message);
	}
	state.loading = false;
}

// A part with its items in review order: by group, most important first within each group. Every
// item carries its partId, which saves and the journal use.
async function loadPart(partId) {
	const part = await api("GET", `/api/parts/${encodeURIComponent(partId)}`);
	for (const item of part.items) item.partId = part.id;
	part.items = byImportance(part.items, (item) => item.group || "");
	return part;
}

// The Key decisions view: the key and important items of every part, shaped like a part so the
// same cards, filters, saves and journal serve it. Groups follow the start page's order: the
// cross-part contradictions of part 08, then parts 01 and 02, then the rest in part order.
async function loadKeyView() {
	const { parts, items } = await loadRatedItems();
	const rank = (part) => {
		const number = partNumber(part);
		return number === "01" ? 1 : number === "02" ? 2 : 3;
	};
	const ordered = parts.filter((part) => !part.error).sort((a, b) => rank(a) - rank(b) || a.order - b.order || a.id.localeCompare(b.id));
	const position = new Map(ordered.map((part, index) => [part.id, index + 1]));
	const label = (part) => `${partNumber(part) ? `Part ${partNumber(part)} · ` : ""}${part.title}`;
	const byId = new Map(parts.map((part) => [part.id, part]));
	const rated = items.filter((item) => position.has(item.partId));
	for (const item of rated) {
		const part = byId.get(item.partId);
		const first = item.partId === FIRST_GROUP.partId && item.group === FIRST_GROUP.group;
		item.viewGroup = first ? `${partNumber(part) ? `Part ${partNumber(part)} · ` : ""}${FIRST_GROUP.group}` : label(part);
		item.viewRank = first ? 0 : position.get(item.partId);
	}
	rated.sort((a, b) => a.viewRank - b.viewRank);
	return {
		id: KEY_VIEW,
		virtual: "key",
		title: "Key decisions",
		status: "ready",
		revision: partsRevision(parts),
		summary: "",
		warnings: [],
		items: byImportance(rated, (item) => item.viewGroup),
	};
}

async function loadOpenView() {
	const { parts, items } = await api("GET", "/api/items");
	const open = items.filter(item => !item.answer);
	open.sort((a, b) => importanceRank(a) - importanceRank(b));
	for (const item of open) item.viewGroup = IMPORTANCE_LABEL[importanceOf(item)] || "Unrated";
	return { id: OPEN_VIEW, virtual: "open", title: "Open questions", revision: partsRevision(parts), items: open };
}

async function reloadPart() {
	if (!state.partId) return;
	flushAll();
	await Promise.allSettled([...state.ui.values()].map((ui) => ui.chain));
	await openPart(state.partId, { keep: true });
}

function leavePart() {
	if (!state.partId && !state.part) return;
	flushAll();
	state.pending = Promise.allSettled([...state.ui.values()].map((ui) => ui.chain));
	state.loadSeq += 1;
	state.loading = false;
	state.part = null;
	state.partId = null;
	state.items = new Map();
	state.ui = new Map();
	state.current = null;
	state.bulk = "idle";
	state.selection.clear();
	state.selectionAsk = null;
	els.cards.replaceChildren();
	els.head.replaceChildren();
	els.toolbar.hidden = true;
	els.listHead.hidden = true;
	document.title = "RichView lockdown review";
}

// Saves one item's answer (null clears it). Saves for one item run in order, so the last click wins.
function commit(item, next) {
	const ui = uiFor(item.id);
	const partId = item.partId;
	const seq = ++ui.seq;
	const updatedAt = item.answer?.updatedAt;
	// Written before the request: a failed send, a closed tab or a reload keeps it.
	const token = J.newToken();
	J.setIntent(store, partId, item.id, next, ui.conflictBase !== undefined ? ui.conflictBase : updatedAt, token);
	ui.conflictBase = undefined;
	ui.conflict = null;
	renderUnsaved();
	item.answer = next
		? { decision: next.decision, text: next.decision === "edit" ? next.text : undefined, note: next.note, stale: false, updatedAt }
		: null;
	ui.intent = next;
	ui.status = "saving";
	updateCard(item);
	renderPartProgress();
	const url = `/api/answers/${encodeURIComponent(partId)}/${encodeURIComponent(item.id)}`;
	const send = () =>
		next
			? api("PUT", url, { decision: next.decision, text: next.decision === "edit" ? next.text : "", note: next.note ?? "" })
			: api("DELETE", url);
	ui.chain = ui.chain.then(send).then(
		(result) => {
			J.settle(store, partId, item.id, token, result.answer?.updatedAt);
			renderUnsaved();
			if (seq !== ui.seq) return;
			item.answer = result.answer ?? null;
			if (ui.noteDraft !== undefined && (item.answer?.note ?? "") === ui.noteDraft.trim()) ui.noteDraft = undefined;
			ui.status = "saved";
			ui.error = "";
			updateCard(item);
			renderPartProgress();
			if (state.partId === OPEN_VIEW && state.items.get(item.id) === item) applyFilters();
			scheduleRefresh();
		},
		(error) => {
			renderUnsaved();
			if (seq !== ui.seq) return;
			ui.status = "error";
			ui.error = error.message;
			updateCard(item);
		},
	);
	return ui.chain;
}

const currentNote = (item) => (uiFor(item.id).noteDraft ?? item.answer?.note ?? "").trim();

function decide(item, decision) {
	if (decision === "edit") {
		openEditor(item);
		return;
	}
	const ui = uiFor(item.id);
	clearTimeout(ui.noteTimer);
	clearTimeout(ui.editTimer);
	ui.noteTimer = 0;
	ui.editTimer = 0;
	ui.editing = false;
	commit(item, { decision, note: currentNote(item) });
}

function clearAnswer(item) {
	const ui = uiFor(item.id);
	clearTimeout(ui.noteTimer);
	clearTimeout(ui.editTimer);
	ui.noteTimer = 0;
	ui.editTimer = 0;
	ui.noteDraft = undefined;
	ui.editDraft = undefined;
	ui.editing = false;
	ui.noteOpen = false;
	J.dropNoteDraft(store, item.partId, item.id);
	commit(item, null);
}

function openEditor(item) {
	const ui = uiFor(item.id);
	if (!ui.editing) {
		ui.editing = true;
		ui.expanded = true;
		ui.open = true;
		ui.editSnapshot = item.answer ? { ...item.answer } : null;
		ui.editDirty = false;
		ui.editArea.value = item.answer?.decision === "edit" ? item.answer.text : (ui.editDraft ?? item.proposed);
		ui.editInitial = ui.editArea.value;
		ui.editHint.textContent = "Saves as you type and replaces the proposal; no Accept needed. Ctrl+Enter saves and closes; Esc closes.";
		ui.editHint.classList.remove("is-error");
		updateCard(item);
	}
	ui.editArea.focus();
	autoGrow(ui.editArea);
}

function onEditInput(item) {
	const ui = uiFor(item.id);
	ui.editDraft = ui.editArea.value;
	autoGrow(ui.editArea);
	// Journaled at once, so a crash inside the autosave delay still keeps the text.
	if (ui.editDraft.trim()) {
		J.setIntent(store, item.partId, item.id, { decision: "edit", text: ui.editDraft, note: currentNote(item) }, item.answer?.updatedAt, "typing");
		renderUnsaved();
	} else {
		dropTyping(item);
	}
	clearTimeout(ui.editTimer);
	ui.editTimer = setTimeout(() => saveEdit(item, false), AUTOSAVE_MS);
}

function saveEdit(item, close) {
	const ui = uiFor(item.id);
	clearTimeout(ui.editTimer);
	ui.editTimer = 0;
	const text = ui.editArea.value;
	if (close && !ui.editDirty && text === ui.editInitial) {
		ui.editing = false;
		updateCard(item);
		focusCard(item);
		return;
	}
	if (!text.trim()) {
		if (close) {
			ui.editHint.textContent = "Write your version, or discard the edit.";
			ui.editHint.classList.add("is-error");
		}
		return;
	}
	const unchanged = item.answer?.decision === "edit" && item.answer.text === text.trimEnd();
	if (unchanged) dropTyping(item);
	else {
		ui.editDirty = true;
		commit(item, { decision: "edit", text, note: currentNote(item) });
	}
	if (close) {
		ui.editing = false;
		updateCard(item);
		focusCard(item);
		if (state.partId === OPEN_VIEW) applyFilters();
	}
}

function discardEdit(item) {
	const ui = uiFor(item.id);
	clearTimeout(ui.editTimer);
	ui.editTimer = 0;
	ui.editing = false;
	ui.editDraft = undefined;
	dropTyping(item);
	if (ui.editDirty) {
		const snapshot = ui.editSnapshot;
		commit(item, snapshot ? { decision: snapshot.decision, text: snapshot.text, note: snapshot.note ?? "" } : null);
	}
	ui.editDirty = false;
	updateCard(item);
	focusCard(item);
}

function openNote(item) {
	const ui = uiFor(item.id);
	ui.noteOpen = true;
	ui.expanded = true;
	ui.open = true;
	updateCard(item);
	ui.noteArea.focus();
	autoGrow(ui.noteArea);
}

function onNoteInput(item) {
	const ui = uiFor(item.id);
	ui.noteDraft = ui.noteArea.value;
	autoGrow(ui.noteArea);
	clearTimeout(ui.noteTimer);
	if (item.answer) {
		J.setIntent(store, item.partId, item.id, { ...item.answer, note: ui.noteDraft }, item.answer.updatedAt, "typing");
		ui.noteTimer = setTimeout(() => saveNote(item), AUTOSAVE_MS);
	} else {
		// No decision yet: the note is kept in this browser and saved with the decision.
		J.setNoteDraft(store, item.partId, item.id, ui.noteDraft);
		ui.noteTimer = 0;
	}
	renderUnsaved();
	updateCard(item);
}

function saveNote(item) {
	const ui = uiFor(item.id);
	clearTimeout(ui.noteTimer);
	ui.noteTimer = 0;
	const answer = item.answer;
	if (!answer || ui.noteDraft === undefined || (answer.note ?? "") === ui.noteDraft.trim()) {
		if (answer) dropTyping(item);
		return;
	}
	commit(item, { decision: answer.decision, text: answer.text, note: ui.noteDraft });
}

// ---------------------------------------------------------------------------------------------
// Journal: unsaved changes kept in this browser, restored into their cards and retried

// Drops a journal entry written while typing when nothing needs saving after all.
function dropTyping(item) {
	const entry = J.get(store, item.partId, item.id);
	if (entry?.intent?.token === "typing") J.dropIntent(store, item.partId, item.id);
	renderUnsaved();
}

const intentNext = (intent) => (intent.clear ? null : { decision: intent.decision, text: intent.text, note: intent.note ?? "" });

function conflictText(intent, answer) {
	const mine = !intent ? "your change" : intent.clear ? "clearing the answer" : intent.decision === "edit" ? "your edit" : `${DECISION_BUTTON[intent.decision]}`;
	const saved = answer ? `${DECISION_STATE[answer.decision].toLowerCase()} at ${formatTime(answer.updatedAt)}` : "cleared";
	return `Not saved: ${mine} from this browser was made before a newer answer (${saved}) was saved elsewhere.`;
}

// The open view's item for a journal entry: the same item ID from the same part, or null.
function openItem(partId, itemId) {
	const item = state.items.get(itemId);
	return item && item.partId === partId ? item : null;
}

// Puts the journal entries of the open view's items back into their cards: drafts into their
// fields, unsaved decisions sent again when the saved answer is still the one they were made on.
function restoreJournal() {
	for (const entry of J.entries(store)) {
		const item = openItem(entry.partId, entry.itemId);
		if (!item) continue;
		const ui = uiFor(item.id);
		if (entry.intent) {
			const verdict = J.classify(entry.intent, item.answer);
			if (verdict === "saved") {
				J.dropIntent(store, entry.partId, item.id);
			} else if (verdict === "retry") {
				if (!entry.intent.clear && entry.intent.note) ui.noteDraft = entry.intent.note;
				commit(item, intentNext(entry.intent));
			} else {
				ui.status = "conflict";
				ui.conflict = entry.intent;
				ui.intent = intentNext(entry.intent);
				ui.open = true;
				ui.expanded = true;
				updateCard(item);
			}
		} else if (entry.noteDraft) {
			ui.noteDraft = entry.noteDraft;
			ui.noteOpen = true;
			updateCard(item);
		}
	}
	renderUnsaved();
}

// Sends unsaved decisions again once the server answers. Changes in the open part go through its
// cards; other parts are checked against the answers the server holds now and sent directly.
async function retryJournal() {
	if (state.retrying || !J.counts(store).unsaved) return;
	state.retrying = true;
	try {
		const byPart = new Map();
		for (const entry of J.entries(store)) {
			if (!entry.intent || entry.intent.token === "typing") continue;
			if (!byPart.has(entry.partId)) byPart.set(entry.partId, []);
			byPart.get(entry.partId).push(entry);
		}
		for (const [partId, list] of byPart) {
			let part;
			try {
				part = await api("GET", `/api/parts/${encodeURIComponent(partId)}`);
			} catch {
				return;
			}
			const saved = new Map(part.items.map((item) => [item.id, item.answer ?? null]));
			for (const entry of list) {
				const open = openItem(partId, entry.itemId);
				const ui = open ? state.ui.get(open.id) : null;
				// A card that is saving, being edited or showing a conflict is left to Daniel.
				if (ui && (ui.status !== "error" || ui.editing || ui.editTimer || ui.noteTimer)) continue;
				const verdict = J.classify(entry.intent, saved.get(entry.itemId));
				if (verdict === "saved") {
					J.dropIntent(store, partId, entry.itemId);
					if (open) {
						open.answer = saved.get(entry.itemId);
						ui.status = "saved";
						ui.error = "";
						updateCard(open);
					}
				} else if (verdict === "conflict") {
					if (open) {
						open.answer = saved.get(entry.itemId);
						ui.status = "conflict";
						ui.conflict = entry.intent;
						updateCard(open);
					}
				} else if (open) {
					commit(open, intentNext(entry.intent));
				} else {
					const url = `/api/answers/${encodeURIComponent(partId)}/${encodeURIComponent(entry.itemId)}`;
					const next = intentNext(entry.intent);
					try {
						const result = next
							? await api("PUT", url, { decision: next.decision, text: next.decision === "edit" ? next.text : "", note: next.note })
							: await api("DELETE", url);
						J.settle(store, partId, entry.itemId, entry.intent.token, result.answer?.updatedAt);
					} catch {
						return;
					}
				}
			}
		}
	} finally {
		state.retrying = false;
		renderUnsaved();
	}
}

function renderUnsaved() {
	if (!els.unsaved) return;
	const { unsaved, notes } = J.counts(store);
	const conflicts = J.entries(store).filter((entry) => entry.intent && openItem(entry.partId, entry.itemId) && state.ui.get(entry.itemId)?.status === "conflict").length;
	const parts = [];
	if (unsaved) parts.push(`${plural(unsaved, "change")} not saved${conflicts ? `, ${conflicts} in conflict` : ""}; kept in this browser and retried when the server answers`);
	if (notes) parts.push(`${plural(notes, "note")} kept in this browser until you decide`);
	els.unsaved.hidden = !parts.length;
	els.unsaved.classList.toggle("is-error", Boolean(unsaved));
	setChildren(els.unsaved, parts.length ? `${parts.join(". ")}.` : "", unsaved ? [" ", el("button", { type: "button", class: "quiet", dataset: { act: "unsaved-show" } }, "Show")] : null);
}

function onUnsavedClick(event) {
	if (!event.target.closest("[data-act='unsaved-show']")) return;
	const first = J.entries(store).find((entry) => entry.intent);
	if (!first) return;
	window.location.hash = `${partHref(first.partId)}/${encodeURIComponent(first.itemId)}`;
}

function onBeforeUnload(event) {
	flushAll();
	if (!J.counts(store).unsaved) return;
	event.preventDefault();
	event.returnValue = "";
}

function flushAll() {
	for (const item of state.items.values()) {
		const ui = state.ui.get(item.id);
		if (ui?.editTimer) saveEdit(item, false);
		if (ui?.noteTimer) saveNote(item);
	}
}

// Open items with a proposal. Accepting in bulk leaves out items with options: each needs an
// explicit Accept, Choose, Reject or Defer of its own.
const remainingItems = () => [...state.items.values()].filter((item) => !item.answer && item.proposed.trim());
const withoutOptions = (items) => items.filter((item) => !hasOptions(item));
const skippedNote = (count) =>
	`${count === 1 ? "1 item with options is" : `${count} items with options are`} skipped: each needs an explicit choice on its card.`;

async function acceptRemaining() {
	const open = remainingItems();
	const list = withoutOptions(open);
	const skipped = open.length - list.length;
	if (!list.length) return;
	const viewId = state.part.id;
	state.bulk = "saving";
	renderPartProgress();
	await Promise.allSettled(list.map((item) => uiFor(item.id).chain));
	const seqs = new Map(list.map((item) => [item.id, uiFor(item.id).seq]));
	const { answers, failed } = await saveBatch(list, "accept");
	if (state.part?.id !== viewId) return;
	for (const item of list) {
		const ui = uiFor(item.id);
		// A click on this item while the batch was out was sent later and wins.
		if (!answers.has(item) || ui.seq !== seqs.get(item.id)) continue;
		item.answer = answers.get(item) ?? item.answer;
		ui.noteDraft = undefined;
		ui.status = "saved";
		updateCard(item);
	}
	if (failed) {
		state.bulk = "error";
		state.bulkError = failed.message;
	} else {
		state.bulk = "idle";
		toast(`Accepted ${plural(list.length, "proposal")}${skipped ? `; ${skipped} with options stay open` : ""}.`);
	}
	renderPartProgress();
	scheduleRefresh();
}

// Saves one decision for several items in one write per part (the Key decisions view spans parts).
// Returns the saved answer of each item whose part saved, and the first error; a part whose batch
// failed saved nothing, and its items are kept as unsaved decisions and retried.
async function saveBatch(list, decision) {
	const byPart = new Map();
	for (const item of list) {
		if (!byPart.has(item.partId)) byPart.set(item.partId, []);
		byPart.get(item.partId).push(item);
	}
	const answers = new Map();
	let failed = null;
	for (const [partId, items] of byPart) {
		try {
			const body = items.map((item) => ({ itemId: item.id, decision, note: currentNote(item) }));
			const result = await api("POST", `/api/answers/${encodeURIComponent(partId)}/bulk`, body);
			for (const item of items) answers.set(item, result.answers[item.id]);
		} catch (error) {
			failed ??= error;
			journalFailedBatch(items, decision, error);
		}
	}
	return { answers, failed };
}

// A failed batch saved nothing; each of its items is kept as an unsaved decision and retried.
function journalFailedBatch(list, decision, error) {
	for (const item of list) {
		const next = { decision, note: currentNote(item) };
		J.setIntent(store, item.partId, item.id, next, item.answer?.updatedAt, J.newToken());
		if (state.items.get(item.id) !== item) continue;
		const ui = uiFor(item.id);
		ui.intent = next;
		ui.status = "error";
		ui.error = error.message;
		updateCard(item);
	}
	renderUnsaved();
}

// ---------------------------------------------------------------------------------------------
// Rows: one dense line per item, a selection, and decisions for several items at once

// The Key decisions view opens as rows, each opening its full card in place, unless Daniel chose cards.
function compactPref(part) {
	const saved = readPref(`rows.${part.virtual || part.id}`);
	return saved === null ? Boolean(part.virtual) || part.items.length > ROWS_OVER : saved === "1";
}

function setCompact(on) {
	if (!state.part) return;
	writePref(`rows.${state.part.virtual || state.part.id}`, on ? "1" : "0");
	if (state.compact === on) return;
	state.compact = on;
	clearSelection();
	for (const ui of state.ui.values()) ui.open = false;
	applyCompact();
	const item = currentVisible();
	if (item) uiFor(item.id).card.scrollIntoView({ block: on ? "nearest" : "start" });
}

function applyCompact() {
	const empty = !state.part || Boolean(state.part.error) || state.items.size === 0;
	els.cards.classList.toggle("is-compact", state.compact);
	els.partView.classList.toggle("is-compact", state.compact);
	els.collapseField.hidden = state.compact;
	els.listHead.hidden = !state.compact || empty;
	for (const button of els.layout.querySelectorAll("[data-layout]")) {
		button.setAttribute("aria-pressed", String((button.dataset.layout === "rows") === state.compact));
	}
	for (const item of state.items.values()) updateCard(item);
	renderSelection();
}

function setOpen(item, open) {
	const ui = uiFor(item.id);
	if (!open && ui.editing) return;
	ui.open = open;
	if (open && ui.history) ui.history.open = true;
	updateCard(item);
	if (!open) {
		const top = ui.card.getBoundingClientRect().top;
		if (top < 0 || top > window.innerHeight - 40) ui.card.scrollIntoView({ block: "nearest" });
	}
}

function setSelected(item, on) {
	const ui = uiFor(item.id);
	if (on) state.selection.add(item.id);
	else state.selection.delete(item.id);
	ui.select.checked = on;
	ui.card.classList.toggle("is-selected", on);
}

function clearSelection() {
	for (const id of [...state.selection]) {
		const item = state.items.get(id);
		if (item) setSelected(item, false);
	}
	state.selection.clear();
	state.selectionAsk = null;
	state.selectionError = "";
	renderSelection();
}

function onSelectClick(item, event) {
	const on = event.target.checked;
	const list = visibleItems();
	const from = list.findIndex((entry) => entry.id === state.selectAnchor);
	const to = list.findIndex((entry) => entry.id === item.id);
	if (event.shiftKey && from !== -1 && to !== -1) {
		for (const entry of list.slice(Math.min(from, to), Math.max(from, to) + 1)) setSelected(entry, on);
	} else {
		setSelected(item, on);
	}
	state.selectAnchor = item.id;
	state.selectionAsk = null;
	renderSelection();
}

function toggleSelected(item) {
	setSelected(item, !state.selection.has(item.id));
	state.selectAnchor = item.id;
	state.selectionAsk = null;
	renderSelection();
}

function selectShown(on) {
	for (const item of visibleItems()) setSelected(item, on);
	state.selectionAsk = null;
	renderSelection();
}

function renderSelection() {
	const shown = state.compact ? visibleItems() : [];
	const picked = shown.filter((item) => state.selection.has(item.id)).length;
	els.selectAll.checked = shown.length > 0 && picked === shown.length;
	els.selectAll.indeterminate = picked > 0 && picked < shown.length;
	els.selectAll.disabled = shown.length === 0;
	els.selectAllLabel.textContent =
		picked === shown.length && picked > 0 ? `All ${shown.length} shown selected` : `Select all ${shown.length} shown`;
	renderSelectionBar();
}

function renderSelectionBar() {
	const count = state.view === "part" && state.compact ? state.selection.size : 0;
	document.body.classList.toggle("has-selection", count > 0);
	els.selectionBar.hidden = count === 0;
	if (!count) return;
	const focused = document.activeElement?.closest?.("#selection-bar [data-act]")?.dataset.act;
	let content;
	if (state.selectionBusy) {
		content = [el("p", { class: "selection-text" }, `Saving ${plural(count, "answer")}…`)];
	} else if (state.selectionAsk) {
		const { decision, changing, edits, skipped } = state.selectionAsk;
		const verb = DECISION_BUTTON[decision];
		const apply = count - skipped;
		const text = [];
		if (skipped) {
			const who = skipped === count ? (count === 1 ? "The selected item has" : `All ${count} selected have`) : `${skipped} of the ${count} selected ${skipped === 1 ? "has" : "have"}`;
			text.push(`${who} options and ${skipped === 1 ? "is" : "are"} skipped: each needs an explicit choice on its card.`);
		}
		if (changing) {
			const edited = edits ? `; ${edits === 1 ? "1 edit loses its text" : `${edits} edits lose their text`}` : "";
			text.push(`${changing} of the ${skipped ? `${apply} to accept` : `${count} selected`} already ${changing === 1 ? "has" : "have"} another answer${edited}.`);
		}
		text.push(apply ? `${verb} ${apply === count ? `all ${count}` : apply}?` : "Nothing is left to accept.");
		content = [
			el("p", { class: "selection-text" }, text.join(" ")),
			el(
				"div",
				{ class: "selection-actions" },
				apply ? actButton(`${verb} ${apply}`, "sel-confirm", "act act-primary") : null,
				actButton(apply ? "Cancel" : "Close", "sel-cancel", "quiet"),
			),
		];
	} else {
		content = [
			el("p", { class: "selection-text" }, el("strong", {}, String(count)), " selected"),
			el(
				"div",
				{ class: "selection-actions" },
				...["accept", "reject", "defer"].map((decision) =>
					actButton(DECISION_BUTTON[decision], `sel-${decision}`, `act act-${decision}`),
				),
			),
			actButton("Clear selection", "sel-clear", "quiet"),
			state.selectionError ? el("p", { class: "save-error" }, `Not saved: ${sentence(state.selectionError)}`) : null,
		];
	}
	setChildren(els.selectionBar, ...content);
	if (focused) els.selectionBar.querySelector(`[data-act="${focused}"]`)?.focus();
}

function onSelectionClick(event) {
	const act = event.target.closest("[data-act]")?.dataset.act;
	if (!act) return;
	if (act === "sel-clear") {
		clearSelection();
		focusCurrent();
	} else if (act === "sel-cancel") {
		state.selectionAsk = null;
		renderSelectionBar();
	} else if (act === "sel-confirm") {
		applySelection(state.selectionAsk.decision, true);
	} else if (act.startsWith("sel-")) {
		applySelection(act.slice(4));
	}
}

async function applySelection(decision, confirmed = false) {
	const selected = [...state.selection].map((id) => state.items.get(id)).filter(Boolean);
	if (!selected.length || state.selectionBusy || !DECISIONS.includes(decision) || decision === "edit") return;
	// Accepting in bulk skips items with options, and always says so first.
	const skipped = decision === "accept" ? selected.filter(hasOptions) : [];
	const list = skipped.length ? withoutOptions(selected) : selected;
	const changing = list.filter((item) => item.answer && item.answer.decision !== decision);
	if (!confirmed && (changing.length || skipped.length)) {
		state.selectionAsk = {
			decision,
			changing: changing.length,
			edits: changing.filter((item) => item.answer.decision === "edit").length,
			skipped: skipped.length,
		};
		renderSelectionBar();
		(els.selectionBar.querySelector("[data-act='sel-confirm']") ?? els.selectionBar.querySelector("[data-act='sel-cancel']"))?.focus();
		return;
	}
	if (!list.length) return;
	const viewId = state.part.id;
	state.selectionAsk = null;
	state.selectionBusy = true;
	state.selectionError = "";
	renderSelectionBar();
	for (const item of list) {
		const ui = uiFor(item.id);
		clearTimeout(ui.editTimer);
		clearTimeout(ui.noteTimer);
		ui.editTimer = 0;
		ui.noteTimer = 0;
		ui.editing = false;
		ui.editDraft = undefined;
	}
	await Promise.allSettled(list.map((item) => uiFor(item.id).chain));
	const seqs = new Map(list.map((item) => [item.id, uiFor(item.id).seq]));
	const { answers, failed } = await saveBatch(list, decision);
	if (failed) state.selectionError = failed.message;
	if (state.part?.id === viewId) {
		for (const item of list) {
			if (!answers.has(item)) continue;
			const ui = uiFor(item.id);
			setSelected(item, false);
			// A click on this item while the batch was out was sent later and wins.
			if (ui.seq !== seqs.get(item.id)) continue;
			item.answer = answers.get(item) ?? item.answer;
			ui.noteDraft = undefined;
			ui.status = "saved";
			ui.error = "";
			updateCard(item);
		}
		if (!failed) {
			for (const item of skipped) setSelected(item, false);
			toast(`${DECISION_STATE[decision]} ${plural(list.length, "item")}${skipped.length ? `; skipped ${skipped.length} with options` : ""}.`);
		}
	}
	state.selectionBusy = false;
	if (state.part?.id !== viewId) return;
	if (state.partId === OPEN_VIEW) applyFilters();
	renderSelection();
	renderPartProgress();
	scheduleRefresh();
}

// ---------------------------------------------------------------------------------------------
// Rendering: masthead and rail

function renderOverall() {
	const totals = state.totals;
	if (!totals) {
		els.overall.replaceChildren();
		return;
	}
	els.overall.replaceChildren(
		el(
			"p",
			{ class: "overall-line" },
			el("strong", {}, String(totals.answeredCount)),
			` of ${totals.itemCount} answered`,
			el("span", { class: "overall-ready" }, `${totals.ready} of ${plural(totals.parts, "part")} ready`),
		),
		bar(totals.answeredCount, totals.itemCount, "overall-bar"),
		el("p", { class: "overall-detail" }, decisionText(totals.decisions)),
	);
}

function renderRail() {
	for (const link of els.rail.querySelectorAll(".dashboard-links a")) {
		const current = link.hash === "#/how-it-works" ? state.view === "technical" || technicalPath(state.doc?.path)
			: link.hash === "#/open" ? state.partId === OPEN_VIEW : state.view === "answered";
		link.setAttribute("aria-current", current ? "page" : "false");
	}
	els.railStart.setAttribute("aria-current", state.view === "start" ? "page" : "false");
	renderRailKey();
	els.railEmpty.hidden = state.parts.length > 0 || Boolean(state.listError) || !state.loaded;
	const entries = state.parts.map((part) => {
		const status = part.error ? "unavailable" : part.status;
		const link = el(
			"a",
			{
				class: "rail-link",
				href: partHref(part.id),
				"aria-current": state.view === "part" && part.id === state.partId ? "page" : null,
			},
			el("span", { class: "rail-number" }, partNumber(part) || "·"),
			el("span", { class: "rail-title" }, part.title),
			el(
				"span",
				{ class: "rail-meta" },
				el("span", { class: `status status-${slug(status)}` }, capitalize(status)),
				part.error ? null : el("span", { class: "rail-count" }, `${part.answeredCount} of ${part.itemCount}`),
				part.error ? null : keyCount(partKeyProgress(part), "rail-key-count"),
				part.staleCount ? el("span", { class: "rail-stale" }, `${part.staleCount} changed`) : null,
			),
			part.error ? el("span", { class: "rail-error" }, shorten(part.error, 96)) : bar(part.answeredCount, part.itemCount, "rail-bar"),
		);
		return el("li", {}, link);
	});
	if (state.listError) entries.push(el("li", { class: "rail-error" }, `Cannot load the parts: ${sentence(state.listError)}`));
	els.railList.replaceChildren(...entries);
	renderGuideRail();
}

// The Key decisions link above the parts, with how many key cards are answered.
function renderRailKey() {
	const key = keyProgress();
	els.railKey.setAttribute("aria-current", state.view === "part" && isKeyView() ? "page" : "false");
	setChildren(
		els.railKey,
		el("span", { class: "rail-title" }, "Key decisions"),
		el(
			"span",
			{ class: "rail-meta" },
			el("span", { class: "rail-count" }, !key ? "Across every part" : key.itemCount ? `${key.answeredCount} of ${key.itemCount} answered` : "No card rated key yet"),
		),
		key?.itemCount ? bar(key.answeredCount, key.itemCount, "rail-bar") : null,
	);
}

function renderGuideRail() {
	const current = state.view === "doc" ? state.doc?.path : null;
	const link = (doc) =>
		el(
			"li",
			{ dataset: { depth: String(Math.min(docDepth(doc.path), 3)) } },
			el("a", { class: "guide-link", href: guideHref(doc.path), "aria-current": doc.path === current ? "page" : null }, doc.title),
		);
	const guide = state.docs.filter((doc) => doc.section === "guide");
	const lockdown = state.docs.filter((doc) => doc.section === "lockdown");
	// Documents in a subfolder of docs/guide sit under a label naming that folder.
	const guideItems = [];
	let folder = "";
	for (const doc of guide) {
		const docFolder = doc.path.split("/").slice(2, -1).join("/");
		if (docFolder && docFolder !== folder) {
			guideItems.push(
				el("li", { class: "guide-folder", dataset: { depth: String(Math.min(docFolder.split("/").length - 1, 3)) } }, `${docFolder}/`),
			);
		}
		folder = docFolder;
		guideItems.push(link(doc));
	}
	setChildren(
		els.guideDocs,
		guide.length ? el("ul", { class: "guide-list" }, ...guideItems) : null,
		lockdown.length ? el("p", { class: "guide-subheading" }, "Lockdown files") : null,
		lockdown.length ? el("ul", { class: "guide-list" }, ...lockdown.map(link)) : null,
		state.docsError ? el("p", { class: "rail-error" }, `Cannot list the documents: ${sentence(state.docsError)}`) : null,
	);
	els.guideEmpty.hidden = guide.length + lockdown.length > 0 || Boolean(state.docsError) || !state.loaded;
	const first = guide[0] ?? lockdown[0];
	els.mastheadGuide.hidden = !first;
	if (first) els.mastheadGuide.href = guideHref(first.path);
	els.mastheadGuide.setAttribute("aria-current", state.view === "doc" && !technicalPath(current) ? "page" : "false");
}

// The answer buttons explained, in a panel beside the cards; ? or the header button opens it.
function toggleHelp(open = els.help.hidden) {
	const inside = els.help.contains(document.activeElement);
	els.help.hidden = !open;
	els.helpToggle.setAttribute("aria-expanded", String(open));
	document.body.classList.toggle("has-help", open);
	if (open) els.help.focus({ preventScroll: true });
	else if (inside && currentVisible()) focusCurrent();
	else if (inside) els.helpToggle.focus();
}

function toggleRail(open = !els.rail.classList.contains("is-open")) {
	els.rail.classList.toggle("is-open", open);
	els.partsToggle.setAttribute("aria-expanded", String(open));
}

// ---------------------------------------------------------------------------------------------
// Rendering: start page

function renderStart() {
	document.title = "RichView lockdown review";
	const guide = state.docs.filter((doc) => doc.section === "guide");
	const readme = state.docs.find((doc) => doc.path === "docs/lockdown/README.md");
	setChildren(
		els.page,
		el(
			"header",
			{ class: "start-head" },
			el("h1", { class: "part-title" }, "Project dashboard"),
			dashboardDoors(),
			el(
				"div",
				{ class: "start-lead" },
				el(
					"p",
					{},
					"The lockdown records the decisions that shape RichView. Each card holds a proposal, its sources and your answer.",
				),
			),
			startKey(),
		),
		el(
			"section",
			{ class: "start-section", "aria-labelledby": "start-decide" },
			el("h2", { class: "start-heading", id: "start-decide" }, "Four answers"),
			el(
				"dl",
				{ class: "legend" },
				legendRow("accept", "Accept", "Keep the proposal exactly as written."),
				legendRow("edit", "Edit", "Replace it with your own wording, in Markdown. It saves as you type."),
				legendRow("reject", "Reject", "Drop it from the rewrite. A note saying why helps the agents."),
				legendRow("defer", "Defer", "Leave it open for a later pass. Agents treat it as unresolved."),
			),
			el(
				"p",
				{ class: "start-note" },
				"Every answer saves at once to ",
				el("code", {}, "docs/lockdown/answers/"),
				". If a proposal changes, its saved decision stays in Answered with a marker. Confidence shows whether the evidence was observed in code, stated in a document, inferred from sources or unknown.",
			),
			el("h3", { class: "start-subheading" }, "On a card with options"),
			el(
				"dl",
				{ class: "legend" },
				legendRow("accept", "Accept", "Takes the recommended option. When the card recommends none, it keeps the proposal as written."),
				legendRow("choose", "Choose", "Picks another option. It saves as your edit, which starts “Chosen option:” and carries that option's detail."),
				legendRow("reject", "Reject", "None of the options: the proposal is dropped."),
				legendRow("defer", "Defer", "Leaves the choice open."),
			),
			el(
				"p",
				{ class: "start-note" },
				"Accepting in bulk, with “Accept all remaining” or on selected rows, skips cards with options: each needs its own answer. In rows they show how many options they have, and decisions are marked too.",
			),
		),
		el(
			"section",
			{ class: "start-section", "aria-labelledby": "start-order" },
			el("h2", { class: "start-heading", id: "start-order" }, "Order"),
			startOrder(),
			el(
				"p",
				{ class: "start-note" },
				"Parts with more than 40 items open as rows, one line per item with checkboxes for deciding several at once; ",
				el("kbd", {}, "v"),
				" switches between rows and cards.",
			),
			startParts(),
		),
		el(
			"section",
			{ class: "start-section", "aria-labelledby": "start-read" },
			el("h2", { class: "start-heading", id: "start-read" }, "Read and export"),
			guide.length
				? el(
						"p",
						{ class: "start-note" },
						"The guide describes RichView as it is: purpose, architecture, components, types, styles, performance and delivery, with screenshots. Start with ",
						el("a", { href: guideHref(guide[0].path) }, guide[0].title),
						guide.length > 1 ? `; ${plural(guide.length - 1, "more document")} in the left rail.` : ".",
					)
				: el(
						"p",
						{ class: "start-note" },
						"The guide in ",
						el("code", {}, "docs/guide/"),
						" is still being written; its documents appear in the left rail as they land.",
					),
			readme
				? el(
						"p",
						{ class: "start-note" },
						el("a", { href: guideHref(readme.path) }, "How the lockdown is organised"),
						": part and answer formats, ID prefixes and the lockdown files.",
					)
				: null,
			el(
				"p",
				{ class: "start-note" },
				...idNodes(
					"Item IDs such as OPN-58 open their card wherever they appear: in cards, sources and guide pages. Repo paths link to GitHub only where they exist: the lockdown branch for docs/lockdown, docs/guide and tools/lockdown-app, otherwise commit b4db233.",
				),
			),
			el(
				"p",
				{ class: "start-note" },
				el("a", { href: "/api/export.md", target: "_blank", rel: "noopener" }, "Export answers"),
				": one Markdown digest of every part and decision, for the agents.",
			),
		),
	);
}

function legendRow(decision, term, text) {
	return el("div", { class: `legend-row is-${decision}` }, el("dt", {}, term), el("dd", {}, text));
}

// Where the cross-part contradiction cards start: their group heading, through their first card.
const firstGroupHref = () => `${partHref(FIRST_GROUP.partId)}/${encodeURIComponent(state.firstGroup?.first ?? FIRST_GROUP.firstItem)}`;

// The order to answer in: the cross-part contradictions decide which part holds where parts
// disagree, so they come before parts 01 and 02, which frame the rest.
function startOrder() {
	const group = state.firstGroup;
	const count = group?.count ?? FIRST_GROUP.count;
	const partLink = (number) => {
		const part = state.parts.find((entry) => partNumber(entry) === number);
		return part ? el("a", { href: partHref(part.id) }, `Part ${number}`) : `Part ${number}`;
	};
	return el(
		"dl",
		{ class: "order" },
		el(
			"div",
			{ class: "order-row is-first" },
			el("dt", {}, "First"),
			el(
				"dd",
				{},
				el("a", { class: "order-link", href: firstGroupHref() }, `The ${count} cross-part contradictions in Part 08`),
				group ? " " : null,
				group ? el("span", { class: "order-progress" }, `${group.answered} of ${group.count} answered`) : null,
				el(
					"p",
					{},
					"Where parts propose incompatible answers, each of these cards decides which one holds. Answer each before the cards it cites; they then follow it.",
				),
			),
		),
		el(
			"div",
			{ class: "order-row" },
			el("dt", {}, "Next"),
			el(
				"dd",
				{},
				partLink("01"),
				" and ",
				partLink("02"),
				el("p", {}, "What RichView is for and what it maximizes, then the delivery model and its gates. They frame every later part."),
			),
		),
		el("div", { class: "order-row" }, el("dt", {}, "Then"), el("dd", {}, "The rest, down the list")),
	);
}

// The shortcut through the lockdown: the key cards of every part, in the order below.
function startKey() {
	const key = keyProgress();
	return el(
		"div",
		{ class: "start-key" },
		el(
			"p",
			{ class: "start-key-line" },
			el("a", { class: "order-link", href: KEY_HREF }, "Key decisions"),
			key ? el("span", { class: "order-progress" }, key.itemCount ? `${key.answeredCount} of ${key.itemCount} answered` : "no card rated key yet") : null,
		),
		el(
			"p",
			{},
			"Start here when time is short: the cards that shape the rewrite most, from every part on one page, in the order below. They answer and save exactly as in their part; widen to key and important cards when the key ones are done. In a part, the Importance filter narrows the same way.",
		),
	);
}

function startParts() {
	if (!state.loaded) return el("p", { class: "start-empty" }, "Loading the parts…");
	if (state.listError) return el("p", { class: "callout callout-alert" }, `Cannot load the parts: ${sentence(state.listError)}`);
	if (!state.parts.length) {
		return el(
			"p",
			{ class: "start-empty" },
			"No part files yet. Agents write them to docs/lockdown/parts/; they appear here within seconds.",
		);
	}
	return el(
		"ol",
		{ class: "start-parts" },
		...state.parts.map((part) => {
			const number = partNumber(part);
			const status = part.error ? "unavailable" : part.status;
			const first = part.id === FIRST_GROUP.partId;
			const next = /^0?[12]$/.test(number);
			return el(
				"li",
				{ class: `start-part${first || next ? " is-first" : ""}` },
				el("span", { class: "rail-number" }, number || "·"),
				el(
					"div",
					{ class: "start-part-main" },
					el("a", { class: "start-part-title", href: partHref(part.id) }, part.title),
					el(
						"p",
						{ class: "start-part-meta" },
						first ? el("a", { class: "start-first", href: firstGroupHref() }, "Its contradictions first") : null,
						next ? el("span", { class: "start-first" }, "Answer next") : null,
						el("span", { class: `status status-${slug(status)}` }, capitalize(status)),
						part.error ? null : el("span", {}, `${part.answeredCount} of ${part.itemCount} answered`),
						part.error ? null : keyCount(partKeyProgress(part), "start-key-count"),
						part.staleCount ? el("span", { class: "rail-stale" }, `${part.staleCount} changed`) : null,
						part.error || !part.answeredCount ? null : el("span", { class: "start-decisions" }, decisionText(part.decisions)),
					),
					part.error ? el("p", { class: "rail-error" }, shorten(part.error, 160)) : null,
				),
				part.error ? null : bar(part.answeredCount, part.itemCount, "start-bar"),
			);
		}),
	);
}

// ---------------------------------------------------------------------------------------------
// Rendering: guide documents

async function openDoc(docPath, anchor) {
	const doc = state.doc;
	if (doc && doc.path === docPath && !doc.loading && (doc.source !== undefined || doc.missing)) {
		if (anchor) scrollToAnchor(anchor);
		else window.scrollTo(0, 0);
		return;
	}
	await loadDoc(docPath, { anchor });
}

async function loadDoc(docPath, { anchor = null, keep = false } = {}) {
	const seq = ++state.docSeq;
	const scrollY = window.scrollY;
	if (keep && state.doc) {
		state.doc.loading = true;
	} else {
		state.doc = { path: docPath, loading: true };
		setChildren(els.page, docHeader(docPath), el("p", { class: "part-eyebrow" }, "Loading the document…"));
	}
	// The server never serves other paths, so they need no request.
	let next = { path: docPath, missing: true };
	if (isDocPath(docPath)) {
		try {
			const response = await fetch(`/api/docs/${encodePath(docPath)}`, { headers: { Accept: "text/markdown" } });
			if (response.ok) {
				next = { path: docPath, source: await response.text(), revision: response.headers.get("X-Doc-Revision") };
			} else if (response.status !== 404) {
				const data = await response.json().catch(() => ({}));
				next = { path: docPath, error: data.error || `${response.status} ${response.statusText}` };
			}
		} catch {
			next = { path: docPath, error: "the app server is not reachable" };
		}
	}
	await linksReady();
	if (seq !== state.docSeq) return;
	state.doc = next;
	renderDoc();
	renderGuideRail();
	if (keep) window.scrollTo(0, scrollY);
	else if (anchor) scrollToAnchor(anchor);
	else window.scrollTo(0, 0);
}

function leaveDoc() {
	state.docSeq += 1;
	state.doc = null;
}

function checkOpenDoc() {
	const doc = state.doc;
	if (!doc || doc.loading) return;
	const entry = state.docs.find((candidate) => candidate.path === doc.path);
	if (!entry) {
		if (doc.source !== undefined) showBanner("This document is no longer on disk; you are reading the last version shown.");
		return;
	}
	hideBanner();
	if (doc.missing || doc.error || entry.revision !== doc.revision) loadDoc(doc.path, { keep: true });
}

function docHeader(docPath) {
	const section = technicalPath(docPath) ? el("a", { href: "#/how-it-works" }, "How it works")
		: docPath.startsWith("docs/guide/") ? "Guide" : "Lockdown file";
	return el(
		"p",
		{ class: "part-eyebrow doc-eyebrow" },
		section,
		el("span", { class: "part-file" }, docPath),
		technicalPath(docPath) ? null : el("a", { class: "doc-github", href: blobUrl(docPath), target: "_blank", rel: "noopener noreferrer" }, "On GitHub"),
	);
}

function renderDoc() {
	const doc = state.doc;
	const entry = state.docs.find((candidate) => candidate.path === doc.path);
	const name = doc.path.split("/").pop();
	if (doc.missing || doc.error) {
		document.title = `${entry?.title ?? name} · RichView lockdown`;
		setChildren(
			els.page,
			docHeader(doc.path),
			el("h1", { class: "part-title" }, entry?.title ?? name),
			doc.missing
				? el(
						"p",
						{ class: "callout" },
						isDocPath(doc.path)
							? "This document is not on disk yet. It appears here as soon as an agent writes it."
							: "Choose a page from How it works or the Guide.",
					)
				: el("p", { class: "callout callout-alert" }, `Cannot open the document: ${sentence(doc.error)}`),
		);
		return;
	}
	const rendered = renderDocument(doc.source, doc.path);
	let contents = rendered.headings.filter(heading => heading.level === 2);
	if (!contents.length && technicalPath(doc.path)) contents = rendered.headings;
	document.title = `${rendered.title || entry?.title || name} · RichView lockdown`;
	setChildren(
		els.page,
		docHeader(doc.path),
		rendered.title
			? el("h1", { class: "part-title doc-title doc-h", id: `doc-${rendered.titleSlug}`, tabindex: "-1", html: rendered.titleHtml })
			: null,
		contents.length >= (technicalPath(doc.path) ? 1 : 3)
			? el(
					"nav",
					{ class: "doc-contents", "aria-label": "Contents" },
					el("p", { class: "label" }, "Contents"),
					el(
						"ol",
						{},
						...contents.map((heading) => el("li", {}, el("a", { href: guideHref(doc.path, heading.slug) }, heading.text))),
					),
				)
			: null,
		el("div", { class: "doc-body md", html: rendered.html }),
	);
}

function scrollToAnchor(anchor) {
	const target = document.getElementById(`doc-${anchor}`) ?? document.getElementById(`doc-${slugify(anchor)}`);
	if (!target) return;
	target.scrollIntoView({ block: "start" });
	target.focus({ preventScroll: true });
}

function onImageError(event) {
	const image = event.target;
	if (!(image instanceof HTMLImageElement) || !image.dataset.path) return;
	const holder = image.closest(".md-image") ?? image;
	holder.replaceWith(el("span", { class: "md-missing" }, `Image not found: ${image.dataset.path}`));
}

// ---------------------------------------------------------------------------------------------
// Rendering: part head and filters

function renderLoading() {
	els.toolbar.hidden = true;
	els.listHead.hidden = true;
	els.cards.replaceChildren();
	els.head.replaceChildren(el("p", { class: "part-eyebrow" }, isKeyView() ? "Loading the key decisions…" : "Loading the part…"));
}

function renderPartError(message) {
	els.toolbar.hidden = true;
	els.listHead.hidden = true;
	els.cards.replaceChildren();
	els.head.replaceChildren(
		el("h1", { class: "part-title" }, isKeyView() ? "The key decisions cannot be opened" : "This part cannot be opened"),
		el("p", { class: "callout callout-alert" }, message),
	);
}

function renderPart() {
	const part = state.part;
	renderHead();
	els.toolbar.hidden = Boolean(part.error) || part.items.length === 0;
	els.partView.classList.toggle("is-key-view", Boolean(part.virtual));
	// The Key decisions view groups by part, and its one decision filter is "Unanswered only".
	els.viewField.hidden = Boolean(part.virtual);
	els.unanswered.closest("label").hidden = part.id === OPEN_VIEW;
	if (part.virtual && !["all", "unanswered"].includes(state.filter.view)) state.filter.view = "all";
	els.view.value = state.filter.view;
	els.unanswered.checked = state.filter.view === "unanswered";
	els.query.value = state.filter.query;
	renderImportanceOptions();
	renderTopicOptions();
	renderKindOptions();
	renderComponentOptions();
	usedDomIds.clear();
	els.cards.replaceChildren(...buildGroups(part.items));
	applyCompact();
	applyFilters();
	document.title = `${part.title} · RichView lockdown`;
}

function renderHead() {
	const part = state.part;
	if (part.id === OPEN_VIEW) {
		setChildren(els.head, el("p", { class: "part-eyebrow" }, "Across every part"),
			el("h1", { class: "part-title" }, part.title),
			el("p", { class: "start-note", id: "part-progress" }));
		renderPartProgress();
		return;
	}
	if (part.virtual) {
		renderKeyHead();
		return;
	}
	const summary = state.parts.find((entry) => entry.id === part.id);
	const status = part.error ? "unavailable" : part.status;
	const number = partNumber(part);
	setChildren(
		els.head,
		el(
			"p",
			{ class: "part-eyebrow" },
			number ? `Part ${number} · ` : "",
			el("span", { class: `status status-${slug(status)}` }, capitalize(status)),
			status === "draft" ? " · may still change" : "",
			el("span", { class: "part-file" }, part.file),
		),
		el("h1", { class: "part-title" }, part.title),
		part.summary ? el("div", { class: "part-summary md", html: renderMarkdown(part.summary) }) : null,
		part.error
			? el(
					"p",
					{ class: "callout callout-alert" },
					`The part file cannot be read (${part.error.replace(/\.$/, "")}). It is probably still being written; this view reloads when the file changes.`,
				)
			: null,
		notices(part, summary),
		part.error ? null : el("div", { class: "part-progress", id: "part-progress" }),
	);
	renderPartProgress();
}

function renderKeyHead() {
	setChildren(
		els.head,
		el("p", { class: "part-eyebrow" }, "Across every part"),
		el("h1", { class: "part-title" }, "Key decisions"),
		el(
			"div",
			{ class: "part-summary" },
			el(
				"p",
				{},
				"The cards that shape the rewrite most, from every part, in the order of the start page: the cross-part contradictions of Part 08, then Parts 01 and 02, then the rest. Open a row to read the full card and answer it there; every answer saves to its own part, as it would in the part.",
			),
		),
		el("div", { class: "part-progress", id: "part-progress" }),
	);
	renderPartProgress();
}

function notices(part, summary) {
	const list = [...(summary?.warnings ?? part.warnings ?? [])];
	if (part.answersError) list.push(part.answersError);
	if (part.orphans?.length) {
		const ids = part.orphans.map((orphan) => `${orphan.itemId} (${orphan.decision ?? "no decision"})`).join(", ");
		list.push(`Saved answers for items no longer in this part: ${ids}. They stay in the answers file and the export.`);
	}
	if (!list.length) return null;
	return el(
		"div",
		{ class: "notices" },
		el("p", { class: "label" }, "About this part file"),
		el("ul", {}, ...list.map((text) => el("li", {}, text))),
	);
}

function renderPartProgress() {
	const box = document.getElementById("part-progress");
	if (!box || !state.part) return;
	if (state.partId === OPEN_VIEW) {
		box.textContent = plural(state.part.items.filter(item => !item.answer).length, "unanswered question");
		return;
	}
	const focused = document.activeElement?.closest?.("#part-progress [data-act]")?.dataset.act;
	const items = scopeItems();
	const tally = tallyAnswers(items);
	const scope = !state.part.virtual ? "" : state.filter.importance === "key" ? " key decisions" : " key and important cards";
	setChildren(
		box,
		el(
			"div",
			{ class: "progress-line" },
			el(
				"p",
				{ class: "progress-text" },
				el("strong", {}, `${tally.answered} of ${items.length}`),
				`${scope} answered`,
				state.part.virtual ? null : keyCount(keyTally(items), "progress-key"),
			),
			el("p", { class: "progress-detail" }, decisionText(tally.decisions)),
		),
		bar(tally.answered, items.length, "part-bar"),
		// Accepting everything at once is for a part; key decisions are answered one by one.
		state.part.virtual ? null : bulkControls(),
	);
	if (focused) box.querySelector(`[data-act="${focused}"]`)?.focus();
	if (state.part.virtual) renderGroupCounts();
}

function keyTally(items) {
	const key = items.filter((item) => importanceOf(item) === "key");
	return { itemCount: key.length, answeredCount: key.filter((item) => item.answer).length };
}

// The items progress counts: a part's every item, or the Key decisions view's importance level.
const scopeItems = () =>
	state.part?.virtual ? [...state.items.values()].filter((item) => importanceIn(item, state.filter.importance)) : [...state.items.values()];

// In the Key decisions view each part's heading counts its answered cards at the chosen importance.
function renderGroupCounts() {
	const groups = new Map();
	for (const item of scopeItems()) {
		const key = groupOf(item);
		const counts = groups.get(key) ?? { answered: 0, total: 0 };
		counts.total += 1;
		if (item.answer) counts.answered += 1;
		groups.set(key, counts);
	}
	for (const section of els.cards.querySelectorAll(".group")) {
		const counts = groups.get(section.dataset.group) ?? { answered: 0, total: 0 };
		const label = section.querySelector(".group-count");
		if (label) label.textContent = state.partId === OPEN_VIEW ? plural(counts.total - counts.answered, "question") : `${counts.answered} of ${counts.total} answered`;
	}
}

function bulkControls() {
	const open = remainingItems();
	const list = withoutOptions(open);
	const skipped = open.length - list.length;
	const box = el("div", { class: "bulk" });
	if (state.bulk === "saving") {
		box.append(el("p", { class: "bulk-text" }, `Accepting ${plural(list.length, "proposal")}…`));
		return box;
	}
	if (!list.length) {
		if (skipped) {
			box.append(el("p", { class: "bulk-note" }, `${skipped === 1 ? "The 1 open item has" : `All ${skipped} open items have`} options, so each needs its own answer.`));
		}
		return box;
	}
	if (state.bulk === "confirm") {
		const conflicts = list.filter((item) => item.conflicts).length;
		const withoutProposal = [...state.items.values()].filter((item) => !item.answer && !item.proposed.trim()).length;
		box.append(
			el(
				"p",
				{ class: "bulk-text" },
				`Accept ${plural(list.length, "remaining proposal")} exactly as written?`,
				conflicts ? ` ${conflicts} of them ${conflicts === 1 ? "has" : "have"} a recorded conflict.` : "",
				skipped ? ` ${skippedNote(skipped)}` : "",
				withoutProposal ? ` ${plural(withoutProposal, "item")} without a proposal ${withoutProposal === 1 ? "stays" : "stay"} open.` : "",
			),
			el("div", { class: "bulk-actions" }, actButton(`Accept ${list.length}`, "bulk-confirm", "act act-primary"), actButton("Cancel", "bulk-cancel", "quiet")),
		);
		return box;
	}
	box.append(
		actButton(
			skipped ? `Accept all remaining proposals without options (${list.length})` : `Accept all remaining proposals in this part (${list.length})`,
			"bulk-start",
			"quiet",
		),
	);
	if (state.bulk === "error") box.append(el("p", { class: "save-error" }, `Not saved: ${sentence(state.bulkError)}`));
	return box;
}

function onHeadClick(event) {
	const act = event.target.closest("[data-act]")?.dataset.act;
	if (act === "bulk-start") state.bulk = "confirm";
	else if (act === "bulk-cancel") state.bulk = "idle";
	else if (act === "bulk-confirm") {
		acceptRemaining();
		return;
	} else return;
	renderPartProgress();
	if (act === "bulk-start") document.querySelector("#part-progress [data-act='bulk-confirm']")?.focus();
}

// Importance: every item, key items only, or key and important ones, with counts. The Key
// decisions view offers the last two.
function renderImportanceOptions() {
	const counts = { key: 0, important: 0 };
	for (const item of state.items.values()) {
		if (importanceOf(item) === "key") counts.key += 1;
		if (importanceIn(item, "important")) counts.important += 1;
	}
	const allowed = isKeyView() ? ["key", "important"] : IMPORTANCE_FILTERS;
	if (!allowed.includes(state.filter.importance)) state.filter.importance = allowed[0];
	setChildren(
		els.importance,
		isKeyView() ? null : el("option", { value: "all" }, "All items"),
		el("option", { value: "key" }, `Key (${counts.key})`),
		el("option", { value: "important" }, `Key + important (${counts.important})`),
	);
	els.importance.value = state.filter.importance;
}

// Topics are the part's group headings, with counts; hidden when the part has one or none.
function renderTopicOptions() {
	const counts = new Map();
	for (const item of state.items.values()) if (item.group) counts.set(item.group, (counts.get(item.group) ?? 0) + 1);
	const show = !state.part.virtual && counts.size > 1;
	if (!show || !counts.has(state.filter.topic)) state.filter.topic = "any";
	els.topicField.hidden = !show;
	setChildren(
		els.topic,
		el("option", { value: "any" }, "Any topic"),
		...(show ? [...counts].map(([topic, count]) => el("option", { value: topic }, `${topic} (${count})`)) : []),
	);
	els.topic.value = state.filter.topic;
}

function renderKindOptions() {
	const counts = new Map();
	for (const item of state.items.values()) counts.set(item.kind, (counts.get(item.kind) ?? 0) + 1);
	if (!counts.has(state.filter.kind)) state.filter.kind = "any";
	els.kind.replaceChildren(
		el("option", { value: "any" }, "Any kind"),
		...[...counts].map(([kind, count]) => el("option", { value: kind }, `${kind} (${count})`)),
	);
	els.kind.value = state.filter.kind;
}

// The components the part's items name, with counts; hidden when no item names one.
function renderComponentOptions() {
	const counts = new Map();
	for (const item of state.items.values()) {
		const key = componentOf(item) || NO_COMPONENT;
		counts.set(key, (counts.get(key) ?? 0) + 1);
	}
	const named = [...counts.keys()].filter((key) => key !== NO_COMPONENT).sort();
	if (!named.length || !counts.has(state.filter.component)) state.filter.component = "any";
	els.componentField.hidden = !named.length;
	setChildren(
		els.component,
		el("option", { value: "any" }, "Any component"),
		...named.map((key) => el("option", { value: key }, `${key} (${counts.get(key)})`)),
		named.length && counts.has(NO_COMPONENT) ? el("option", { value: NO_COMPONENT }, `No component (${counts.get(NO_COMPONENT)})`) : null,
	);
	els.component.value = state.filter.component;
}

// Shows only one component's items; a second click on the same chip shows every item again.
function filterByComponent(item, component) {
	state.filter.component = state.filter.component === component ? "any" : component;
	els.component.value = state.filter.component;
	applyFilters();
	setCurrent(item.id, { scroll: true });
}

function applyFilters() {
	if (!state.part) return;
	const { view, kind, component, topic, importance, query } = state.filter;
	let shown = 0;
	for (const item of state.items.values()) {
		const ui = uiFor(item.id);
		const answer = item.answer;
		const visible =
			(state.partId !== OPEN_VIEW || !answer || ui.editing || ui.status !== "saved") &&
			(view === "all" ||
				(view === "unanswered" && !answer) ||
				(DECISIONS.includes(view) && answer?.decision === view) ||
				(view === "conflicts" && Boolean(item.conflicts)) ||
				(view === "stale" && Boolean(answer?.stale))) &&
			importanceIn(item, importance) &&
			(topic === "any" || item.group === topic) &&
			(kind === "any" || item.kind === kind) &&
			(component === "any" || (componentOf(item) || NO_COMPONENT) === component) &&
			(!query || searchText(item).includes(query));
		for (const button of ui.componentChips ?? []) button.setAttribute("aria-pressed", String(componentOf(item) === component));
		ui.card.hidden = !visible;
		if (visible) shown += 1;
		// A selection only ever holds items you can see, so a decision never lands on a hidden one;
		// a pending confirmation counted the old selection, so it closes.
		else if (state.selection.has(item.id)) {
			setSelected(item, false);
			state.selectionAsk = null;
		}
	}
	for (const group of els.cards.querySelectorAll(".group")) group.hidden = !group.querySelector(".card:not([hidden])");
	// The Key decisions view counts against its importance level, a part against all its items.
	const total = state.part.virtual ? scopeItems().length : state.items.size;
	els.showing.textContent = shown === total ? "" : `Showing ${shown} of ${total}`;
	els.showing.hidden = shown === total;
	els.cards.querySelector(".no-match")?.remove();
	const empty = shown ? "" : noMatchText();
	if (empty) els.cards.append(el("p", { class: "no-match" }, empty));
	if (state.part.virtual) renderGroupCounts();
	renderSelection();
}

function noMatchText() {
	if (state.partId === OPEN_VIEW) return state.part.items.some(item => !item.answer) ? "No questions match these filters." : "Every question has an answer.";
	const rated = [...state.items.values()].some((item) => importanceOf(item));
	if (state.part.virtual) {
		if (!state.items.size) {
			return "No card is rated key or important yet. Agents rate cards with importance in docs/lockdown/parts/; rated cards show up here within seconds.";
		}
		const scope = scopeItems();
		if (!scope.length) return "No card is rated key yet. Widen Importance to Key + important.";
		if (state.filter.view === "unanswered" && scope.every((item) => item.answer)) return "Every card at this importance is answered.";
	} else if (!state.items.size) {
		return "";
	} else if (state.filter.importance !== "all" && !rated) {
		return "No item in this part is rated yet. Set Importance to All items to see every item.";
	}
	return "No items match these filters.";
}

function searchText(item) {
	const ui = uiFor(item.id);
	ui.search ??= [
		item.id,
		item.refId,
		componentOf(item),
		item.kind,
		item.group,
		item.title,
		item.question,
		item.why,
		item.proposed,
		item.recommendation,
		item.conflicts,
		...item.options.flatMap((option) => [option.label, option.detail]),
		...item.sources.flatMap((source) => [source.label, source.ref, source.date]),
	]
		.join("\n")
		.toLowerCase();
	return `${ui.search}\n${item.answer?.text ?? ""}\n${item.answer?.note ?? ""}`.toLowerCase();
}

// ---------------------------------------------------------------------------------------------
// Rendering: cards and rows

function uiFor(itemId) {
	let ui = state.ui.get(itemId);
	if (!ui) {
		ui = {
			chain: Promise.resolve(),
			seq: 0,
			status: "idle",
			error: "",
			intent: null,
			expanded: false,
			open: false,
			editing: false,
			noteOpen: false,
		};
		state.ui.set(itemId, ui);
	}
	return ui;
}

function buildGroups(items) {
	const groups = new Map();
	for (const item of items) {
		const key = groupOf(item);
		if (!groups.has(key)) groups.set(key, []);
		groups.get(key).push(item);
	}
	const named = [...groups.keys()].some(Boolean);
	return [...groups].map(([name, members]) =>
		el(
			"section",
			{ class: "group", "aria-label": name || "Items", dataset: { group: name } },
			named
				? el("h2", { class: "group-title" }, name || "Other", el("span", { class: "group-count" }, plural(members.length, "item")))
				: null,
			...members.map(buildCard),
		),
	);
}

const usedDomIds = new Set();

function buildCard(item) {
	const ui = uiFor(item.id);
	let id = `item-${slug(item.id)}`;
	while (usedDomIds.has(id)) id += "-x";
	usedDomIds.add(id);
	const confidence = slug(item.confidence);
	ui.buttons = {};
	ui.componentChips = [];
	ui.stateLabel = el("p", { class: "decision-state" });
	ui.saveLabel = el("p", { class: "save-state" });
	ui.toggle = el("button", { type: "button", class: "quiet toggle", dataset: { act: "toggle" }, hidden: true }, "Show details");
	ui.staleNote = el(
		"p",
		{ class: "callout callout-alert detail", hidden: true },
		"The proposal changed after your answer. Read it again and answer again.",
	);
	ui.history = item.answer?.stale ? historyDisclosure(item) : null;
	ui.versionBody = el("div", { class: "md serif" });
	ui.versionLabel = el("p", { class: "label" }, "Your version");
	ui.version = el("div", { class: "your-version detail", hidden: true }, ui.versionLabel, ui.versionBody);
	ui.recommended = recommendedIndex(item);
	ui.editArea = el("textarea", { id: `${id}-edit`, class: "text-input", rows: "6", spellcheck: "true", dataset: { role: "edit" } });
	ui.editHint = el("p", { class: "hint" });
	ui.editor = el(
		"div",
		{ class: "editor detail", hidden: true },
		el("label", { class: "label", for: `${id}-edit` }, "Your version (Markdown)"),
		ui.editArea,
		el("div", { class: "editor-actions" }, actButton("Save edit", "save-edit", "act act-primary"), actButton("Discard edit", "discard-edit", "quiet")),
		ui.editHint,
	);
	ui.noteArea = el("textarea", { id: `${id}-note`, class: "text-input", rows: "2", dataset: { role: "note" } });
	ui.noteHint = el("p", { class: "hint" }, "Kept in this browser until you decide, then saved together with your decision.");
	ui.note = el(
		"div",
		{ class: "note detail", hidden: true },
		el("label", { class: "label", for: `${id}-note` }, "Note for the agent"),
		ui.noteArea,
		ui.noteHint,
	);
	ui.noteButton = setHelp(actButton("Add note", "note", "quiet"), NOTE_HELP);
	ui.clearButton = setHelp(actButton("Clear answer", "clear", "quiet"), CLEAR_HELP);
	ui.errorText = el("span");
	ui.errorLine = el("p", { class: "save-error", role: "alert", hidden: true }, ui.errorText, " ", actButton("Retry", "retry", "quiet"));
	ui.conflictText = el("span");
	ui.conflictLine = el(
		"p",
		{ class: "save-error save-conflict", role: "alert", hidden: true },
		ui.conflictText,
		" ",
		actButton("Use my unsaved change", "use-mine", "quiet"),
		" ",
		actButton("Keep the saved answer", "keep-saved", "quiet"),
	);

	const help = buttonHelp(item, ui);
	const decisions = el(
		"div",
		{ class: "decisions", role: "group", "aria-label": `Decision for ${item.id}` },
		...DECISIONS.map((decision) => {
			ui.buttons[decision] = setHelp(
				el("button", { type: "button", class: `act act-${decision}`, dataset: { act: decision }, "aria-pressed": "false" }, DECISION_BUTTON[decision]),
				help[decision],
			);
			return ui.buttons[decision];
		}),
	);

	ui.card = el(
		"article",
		{ class: "card", id, tabindex: "-1", "aria-labelledby": `${id}-title`, dataset: { item: item.id } },
		buildRow(item, ui, id),
		el(
			"div",
			{ class: "card-margin" },
			el(
				"div",
				{ class: "chips" },
				chip(item.id, "item ID"),
				item.refId ? chip(item.refId, "entity ID", "chip-ref") : null,
				componentChip(item, ui),
			),
			importanceBadge(item),
			el("p", { class: "kind detail" }, item.kind),
			el(
				"p",
				{ class: `confidence confidence-${confidence} detail`, title: CONFIDENCE_HELP[confidence] ?? item.confidence },
				capitalize(item.confidence),
			),
			el("div", { class: "card-status" }, ui.stateLabel, ui.saveLabel),
		),
		el(
			"div",
			{ class: "card-main" },
			el("div", { class: "card-heading" }, el("h3", { class: "card-title", id: `${id}-title` }, titleNodes(item.title)), ui.toggle),
			ui.staleNote,
			item.question ? el("div", { class: "question md detail", html: renderMarkdown(item.question) }) : null,
			item.why ? block("Why it matters", renderMarkdown(item.why), "why") : null,
			ui.history,
			block(
				item.answer?.stale ? "Current proposal" : "Proposed answer",
				item.proposed ? renderMarkdown(item.proposed) : '<p class="empty">No proposal. Edit to write the answer, or defer.</p>',
				"proposal",
				"serif",
			),
			ui.version,
			ui.editor,
			item.options.length ? optionsNode(item, ui) : null,
			item.recommendation && !item.options.length ? block("Recommendation", renderMarkdown(item.recommendation), "recommendation") : null,
			item.conflicts ? block("Conflict", renderMarkdown(item.conflicts), "conflict") : null,
			item.sources.length ? sourcesNode(item) : null,
			item.images.length ? imagesNode(item) : null,
			ui.note,
			el(
				"div",
				{ class: "actions detail" },
				decisions,
				el("div", { class: "secondary" }, ui.noteButton, ui.clearButton),
			),
			ui.errorLine,
			ui.conflictLine,
		),
	);
	return ui.card;
}

// The dense line shown in rows mode; the full card stays in the DOM and opens in place.
function buildRow(item, ui, id) {
	const confidence = slug(item.confidence);
	ui.select = el("input", { type: "checkbox", class: "row-check", dataset: { role: "select" }, "aria-label": `Select ${item.id}` });
	ui.rowTitle = el("button", { type: "button", class: "row-title", id: `${id}-row`, dataset: { act: "open" }, "aria-expanded": "false" }, titleNodes(item.title));
	ui.rowExcerpt = el("p", { class: "row-excerpt" });
	ui.rowState = el("span", { class: "decision-state" });
	ui.rowStale = el("span", { class: "row-stale", hidden: true }, "changed");
	ui.rowSave = el("span", { class: "save-state" });
	ui.rowRetry = el("button", { type: "button", class: "quiet row-retry", dataset: { act: "retry" }, hidden: true }, "Retry");
	ui.rowButtons = {};
	return el(
		"div",
		{ class: "row" },
		el("div", { class: "row-select" }, ui.select),
		// The item ID shares a line with its status; the entity and component IDs, often long, get their own.
		el(
			"div",
			{ class: "row-label" },
			el(
				"div",
				{ class: "row-meta" },
				chip(item.id, "item ID"),
				importanceBadge(item),
				el("span", { class: `confidence confidence-${confidence}`, title: CONFIDENCE_HELP[confidence] ?? item.confidence }, capitalize(item.confidence)),
				ui.rowState,
				ui.rowStale,
				ui.rowSave,
				ui.rowRetry,
			),
			// Decisions and options stay visible in a row: an option needs an explicit choice.
			item.kind === "decision" || hasOptions(item)
				? el(
						"div",
						{ class: "row-flags" },
						item.kind === "decision" ? el("span", { class: "row-flag" }, "decision") : null,
						hasOptions(item) ? el("span", { class: "row-flag row-flag-options" }, plural(item.options.length, "option")) : null,
					)
				: null,
			item.refId || componentOf(item)
				? el("div", { class: "row-ref" }, item.refId ? chip(item.refId, "entity ID", "chip-ref") : null, componentChip(item, ui))
				: null,
		),
		el("div", { class: "row-text" }, ui.rowTitle, ui.rowExcerpt),
		el(
			"div",
			{ class: "row-actions decisions", role: "group", "aria-label": `Decision for ${item.id}` },
			...DECISIONS.map((decision) => {
				ui.rowButtons[decision] = setHelp(
					el("button", { type: "button", class: `act act-${decision}`, dataset: { act: decision }, "aria-pressed": "false" }, DECISION_BUTTON[decision]),
					buttonHelp(item, ui)[decision],
				);
				return ui.rowButtons[decision];
			}),
		),
	);
}

function updateCard(item) {
	const ui = state.ui.get(item.id);
	if (!ui?.card) return;
	const answer = item.answer;
	const decision = answer?.decision ?? null;
	const compact = state.compact;
	// A choice is stored as an edit but shows as the option it chose, not as Edit pressed.
	const chosen = chosenIndex(item);
	const choice = chosen === -1 ? null : item.options[chosen];
	ui.card.classList.toggle("is-answered", Boolean(answer));
	ui.card.classList.toggle("is-chosen", Boolean(choice));
	for (const option of DECISIONS) {
		const pressed = decision === option && !(option === "edit" && choice);
		ui.card.classList.toggle(`is-${option}`, decision === option);
		ui.buttons[option].setAttribute("aria-pressed", String(pressed));
		ui.rowButtons[option].setAttribute("aria-pressed", String(pressed));
	}
	if (ui.options) updateOptions(item, ui, chosen);
	const collapsible = !compact && state.collapse && Boolean(answer) && !ui.editing;
	ui.card.classList.toggle("is-collapsed", collapsible && !ui.expanded);
	ui.card.classList.toggle("is-row", compact && !ui.open);
	ui.card.classList.toggle("is-open", compact && Boolean(ui.open));
	ui.toggle.hidden = !(collapsible || (compact && ui.open && !ui.editing));
	ui.toggle.textContent = compact ? "Back to the row" : ui.expanded ? "Hide details" : "Show details";
	ui.toggle.setAttribute("aria-expanded", String(compact ? Boolean(ui.open) : Boolean(ui.expanded)));
	ui.rowTitle.setAttribute("aria-expanded", String(compact && Boolean(ui.open)));
	// An answered item reads as done at a glance: a badge naming what it holds. Open ones say Open.
	const choiceName = choice ? plainExcerpt(optionName(choice), 200) : "";
	ui.stateLabel.textContent = choice ? `Chosen: ${shorten(choiceName, 64)}` : decision ? DECISION_BADGE[decision] : "Open";
	ui.stateLabel.className = `decision-state is-${choice ? "chosen" : (decision ?? "open")}`;
	ui.stateLabel.title = choice ? `Chosen: ${choiceName}` : "";
	ui.rowState.textContent = choice ? `Chosen: ${shorten(choiceName, 28)}` : ui.stateLabel.textContent;
	ui.rowState.className = ui.stateLabel.className;
	ui.rowState.title = ui.stateLabel.title;
	// Accept on an edit or choice would replace it; its tooltip says so.
	const acceptHelp = decision === "edit" ? `Replaces your ${choice ? "choice" : "edit"} with ${acceptsWhat(item, ui)}.` : buttonHelp(item, ui).accept;
	for (const button of [ui.buttons.accept, ui.rowButtons.accept]) setHelp(button, acceptHelp);

	const edited = decision === "edit" && answer.text;
	ui.version.hidden = !edited || ui.editing;
	ui.versionLabel.textContent = choice ? "Your choice" : "Your version";
	if (edited && ui.versionSource !== answer.text) {
		ui.versionBody.innerHTML = renderMarkdown(answer.text);
		ui.versionSource = answer.text;
	}
	// The row shows the chosen option, your edit, or the proposal.
	const excerptKind = choice ? "choice" : edited ? "edit" : "proposal";
	const excerptSource = choice ? optionName(choice) : edited ? answer.text : item.proposed;
	if (ui.excerptSource !== excerptSource || ui.excerptKind !== excerptKind) {
		ui.excerptSource = excerptSource;
		ui.excerptKind = excerptKind;
		setChildren(
			ui.rowExcerpt,
			excerptKind === "proposal" ? null : el("span", { class: "row-excerpt-label" }, choice ? "Your choice: " : "Your version: "),
			excerptSource ? plainExcerpt(excerptSource) : "No proposal. Open the row to write one, or defer.",
		);
		ui.rowExcerpt.classList.toggle("is-empty", !excerptSource);
	}
	ui.staleNote.hidden = !answer?.stale;
	if (ui.history) ui.history.hidden = !answer?.stale;
	ui.rowStale.hidden = !answer?.stale;
	ui.editor.hidden = !ui.editing;
	ui.buttons.edit.textContent = decision === "edit" && !choice && !ui.editing ? "Edit again" : "Edit";

	const note = ui.noteDraft ?? answer?.note ?? "";
	ui.note.hidden = !(ui.noteOpen || note);
	if (document.activeElement !== ui.noteArea && ui.noteArea.value !== note) ui.noteArea.value = note;
	ui.noteHint.hidden = Boolean(answer);
	ui.noteButton.hidden = !ui.note.hidden;
	ui.clearButton.hidden = !answer;

	ui.errorLine.hidden = ui.status !== "error";
	ui.rowRetry.hidden = ui.status !== "error";
	ui.conflictLine.hidden = ui.status !== "conflict";
	if (ui.status === "conflict") ui.conflictText.textContent = conflictText(ui.conflict, answer);
	ui.card.classList.toggle("has-error", ui.status === "error" || ui.status === "conflict");
	ui.saveLabel.classList.toggle("is-error", ui.status === "error" || ui.status === "conflict");
	if (ui.status === "saving") ui.saveLabel.textContent = "Saving…";
	else if (ui.status === "error") ui.saveLabel.textContent = "Not saved";
	else if (ui.status === "conflict") ui.saveLabel.textContent = "Not saved: conflict";
	else if (answer?.updatedAt) {
		// An edit or a choice is the whole answer; the card says so, so nothing is accepted on top.
		const what = choice ? " as your choice" : decision === "edit" ? " as your edit" : "";
		ui.saveLabel.textContent = `Saved${what} ${formatTime(answer.updatedAt)}`;
	}
	else ui.saveLabel.textContent = "";
	ui.rowSave.textContent = ["saving", "error", "conflict"].includes(ui.status) ? ui.saveLabel.textContent : "";
	ui.rowSave.className = ui.saveLabel.className;
	if (ui.status === "error") ui.errorText.textContent = `Not saved: ${sentence(ui.error)}`;
}

function block(label, html, kind, extraClass = "") {
	return el(
		"div",
		{ class: `${kind} detail` },
		el("p", { class: "label" }, label),
		el("div", { class: `md ${extraClass}`.trim(), html }),
	);
}

// The options as text, each with a Choose button; updateCard marks the recommended, accepted and
// chosen ones. Labels wrap like prose: one can run to 150 characters of item IDs.
function optionsNode(item, ui) {
	ui.options = item.options.map((option, index) => {
		const name = optionName(option);
		const mark = el("span", { class: "option-mark" });
		const button = setHelp(
			el(
				"button",
				{ type: "button", class: "act act-choose", dataset: { act: "choose", option: String(index) }, "aria-pressed": "false", "aria-label": `Choose ${plainExcerpt(name, 120)}` },
				"Choose",
			),
			CHOOSE_HELP,
		);
		const row = el(
			"li",
			{ class: `option${index === ui.recommended ? " is-recommended" : ""}` },
			el(
				"div",
				{ class: "option-text" },
				el("p", { class: "option-label" }, el("span", { class: "md", html: renderInline(option.label || option.detail) }), mark),
				option.label && option.detail ? el("div", { class: "option-detail md", html: renderInline(option.detail) }) : null,
			),
			button,
		);
		return { row, button, mark };
	});
	return el(
		"div",
		{ class: "options detail" },
		el("p", { class: "label" }, "Options"),
		el(
			"p",
			{ class: "options-hint" },
			ui.recommended === -1
				? "Choose an option, or Accept keeps the proposal as written; none is recommended. Choosing is your whole answer."
				: "Choose an option, or Accept takes the recommended one. Choosing is your whole answer.",
		),
		el("ul", { class: "option-list" }, ...ui.options.map((entry) => entry.row)),
		item.recommendation
			? el("div", { class: "recommendation" }, el("p", { class: "label" }, "Recommendation"), el("div", { class: "md", html: renderMarkdown(item.recommendation) }))
			: null,
	);
}

// Marks each option: recommended, taken by Accept, or chosen, with its Choose button pressed.
function updateOptions(item, ui, chosen) {
	const accepted = item.answer?.decision === "accept";
	ui.options.forEach((entry, index) => {
		const recommended = index === ui.recommended;
		const isChosen = index === chosen;
		entry.row.classList.toggle("is-chosen", isChosen);
		entry.row.classList.toggle("is-accepted", accepted && recommended);
		entry.button.setAttribute("aria-pressed", String(isChosen));
		entry.button.textContent = isChosen ? "Chosen" : "Choose";
		entry.mark.textContent = [recommended ? "recommended" : "", isChosen ? "your choice" : accepted && recommended ? "accepted" : ""]
			.filter(Boolean)
			.join(" · ");
	});
}

// An option without a label is named by the start of its detail.
const optionName = (option) => option.label || plainExcerpt(option.detail, 120);
const choiceLine = (option) => `${CHOICE_PREFIX}${optionName(option)}${/[.!?…]$/.test(optionName(option)) ? "" : "."}`;
const choiceText = (option) => (option.detail ? `${choiceLine(option)}\n\n${option.detail}` : choiceLine(option));
const hasOptions = (item) => item.options.length > 0;

// The option an answer chose: an edit whose first line is that option's "Chosen option" line.
function chosenIndex(item) {
	const answer = item.answer;
	if (answer?.decision !== "edit" || typeof answer.text !== "string" || !hasOptions(item)) return -1;
	const first = answer.text.split("\n", 1)[0].trim();
	return item.options.findIndex((option) => choiceLine(option) === first);
}

// The option the recommendation names: the part format has it start with that option's label. The
// longest label that starts it as whole words wins; -1 when none does.
function recommendedIndex(item) {
	const recommendation = item.recommendation.trim().toLowerCase();
	let best = -1;
	item.options.forEach((option, index) => {
		const label = option.label.toLowerCase();
		const named = label && recommendation.startsWith(label) && !/[\p{L}\p{N}]/u.test(recommendation.charAt(label.length));
		if (named && (best === -1 || label.length > item.options[best].label.length)) best = index;
	});
	return best;
}

// Choosing saves an edit that names the option and carries its detail, so the export reads as the decision.
function choose(item, index) {
	const option = item.options[index];
	if (!option) return;
	const text = choiceText(option);
	if (item.answer?.decision === "edit" && item.answer.text === text && !item.answer.stale) return;
	const ui = uiFor(item.id);
	clearTimeout(ui.noteTimer);
	clearTimeout(ui.editTimer);
	ui.noteTimer = 0;
	ui.editTimer = 0;
	ui.editing = false;
	ui.editDraft = undefined;
	commit(item, { decision: "edit", text, note: currentNote(item) });
}

function sourcesNode(item) {
	return el(
		"div",
		{ class: "sources detail" },
		el("p", { class: "label" }, "Sources"),
		el(
			"ul",
			{ class: "source-list" },
			...item.sources.map((source) =>
				el(
					"li",
					{ class: "source" },
					source.label ? el("span", { class: "source-label" }, ...idNodes(source.label)) : null,
					source.ref ? el("span", { class: "source-ref" }, ...refNodes(source.ref)) : null,
					source.date ? el("span", { class: "source-date" }, source.date) : null,
				),
			),
		),
	);
}

function imagesNode(item) {
	const figures = item.images.map((ref) => {
		if (/^https?:\/\//i.test(ref)) return el("p", { class: "shot-external" }, externalLink(ref, ref));
		const repoPath = ref.replace(/^\.?\//, "").replace(/^screenshots\//, "docs/lockdown/screenshots/");
		const src = `/files/${encodePath(repoPath)}`;
		const name = repoPath.split("/").pop();
		const figure = el("figure", { class: "shot" });
		if (repoPath.split("/").includes("..")) {
			figure.classList.add("is-missing");
			figure.append(el("figcaption", {}, el("span", { class: "shot-missing" }, `Not found: ${repoPath}`)));
			return figure;
		}
		const image = el("img", { src, alt: `Screenshot ${name}`, loading: "lazy", decoding: "async" });
		image.addEventListener("error", () => figure.classList.add("is-missing"), { once: true });
		figure.append(
			el(
				"button",
				{ type: "button", class: "shot-toggle", dataset: { act: "zoom" }, "aria-expanded": "false", "aria-label": `Enlarge ${name}` },
				image,
			),
			el(
				"figcaption",
				{},
				el("span", { class: "shot-name" }, name),
				el("span", { class: "shot-missing" }, `Not found: ${repoPath}`),
				externalLink(src, "Open original"),
			),
		);
		return figure;
	});
	return el("div", { class: "images detail" }, el("p", { class: "label" }, "Screenshots"), el("div", { class: "shots" }, ...figures));
}

function chip(value, what, extraClass = "") {
	return el(
		"button",
		{ type: "button", class: `chip ${extraClass}`.trim(), dataset: { copy: value }, title: `Copy ${what} ${value}`, "aria-label": `Copy ${what} ${value}` },
		value,
	);
}

// The owning component's ID; clicking it filters the part to that component (filterByComponent).
function componentChip(item, ui) {
	const component = componentOf(item);
	if (!component) return null;
	const button = el(
		"button",
		{
			type: "button",
			class: "chip chip-component",
			dataset: { act: "component", component },
			"aria-pressed": String(state.filter.component === component),
			title: `Show only ${component} items; click again to show all`,
			"aria-label": `Filter by component ${component}`,
		},
		component,
	);
	ui.componentChips.push(button);
	return button;
}

// One sentence on each answer button, as a tooltip and for screen readers.
const CHOOSE_HELP = "Pick this option; that is your full answer.";
const NOTE_HELP = "A comment for the agents, not an answer; on an undecided card it is saved with your next decision.";
const CLEAR_HELP = "Remove your decision.";
const acceptsWhat = (item, ui) => (hasOptions(item) && ui.recommended !== -1 ? "the recommended option" : "the proposal as written");

function buttonHelp(item, ui) {
	return {
		accept: hasOptions(item) && ui.recommended !== -1 ? "Take the recommended option." : "Keep the proposal as written.",
		edit: "Write your own wording: it saves as you type and replaces the proposal, so no Accept is needed afterwards.",
		reject: "Drop it: the rewrite does not build or follow it.",
		defer: "Decide later; it stays open.",
	};
}

function setHelp(button, text) {
	button.title = text;
	button.setAttribute("aria-description", text);
	return button;
}

// Key, Important or Detail; an unrated item shows none. Only Key stands out.
function importanceBadge(item) {
	const level = importanceOf(item);
	return level ? el("span", { class: `importance importance-${level}`, title: IMPORTANCE_HELP[level] }, IMPORTANCE_LABEL[level]) : null;
}

function actButton(label, act, className) {
	return el("button", { type: "button", class: className, dataset: { act } }, label);
}

// One line of plain text from a Markdown proposal, for the row excerpt.
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

// ---------------------------------------------------------------------------------------------
// Card interaction

function itemOf(target) {
	const card = target instanceof Element ? target.closest(".card") : null;
	return card ? state.items.get(card.dataset.item) : undefined;
}

function onCardClick(event) {
	const item = itemOf(event.target);
	if (!item) return;
	if (state.current !== item.id) setCurrent(item.id);
	const ui = uiFor(item.id);
	if (event.target.closest(".row-select")) {
		if (event.target.matches("input[data-role='select']")) onSelectClick(item, event);
		return;
	}
	const button = event.target.closest("button");
	if (!button) {
		// A click on the empty part of a row opens it, unless it was the end of a text selection.
		if (state.compact && !ui.open && event.target.closest(".row") && !event.target.closest("a") && !String(window.getSelection())) {
			setOpen(item, true);
		}
		return;
	}
	if (button.dataset.copy) {
		copyValue(button, button.dataset.copy);
		return;
	}
	switch (button.dataset.act) {
		case "accept":
		case "reject":
		case "defer":
			decide(item, button.dataset.act);
			break;
		case "edit":
			openEditor(item);
			break;
		case "choose":
			choose(item, Number(button.dataset.option));
			break;
		case "save-edit":
			saveEdit(item, true);
			break;
		case "discard-edit":
			discardEdit(item);
			break;
		case "note":
			openNote(item);
			break;
		case "clear":
			clearAnswer(item);
			toast(`Cleared the answer to ${item.id}.`);
			break;
		case "retry":
			commit(item, ui.intent);
			break;
		case "use-mine":
			if (ui.conflict) {
				ui.conflictBase = item.answer?.updatedAt ?? null;
				commit(item, ui.conflict.clear ? null : ui.conflict);
			}
			break;
		case "keep-saved":
			J.dropIntent(store, item.partId, item.id);
			ui.conflict = null;
			ui.status = "idle";
			ui.noteDraft = undefined;
			renderUnsaved();
			updateCard(item);
			break;
		case "open":
			setOpen(item, !ui.open);
			break;
		case "component":
			filterByComponent(item, button.dataset.component);
			break;
		case "toggle":
			if (state.compact) {
				setOpen(item, false);
				focusCard(item);
			} else {
				ui.expanded = !ui.expanded;
				if (ui.expanded && ui.history) ui.history.open = true;
				updateCard(item);
			}
			break;
		case "zoom": {
			const figure = button.closest(".shot");
			const large = figure.classList.toggle("is-large");
			button.setAttribute("aria-expanded", String(large));
			button.setAttribute("aria-label", `${large ? "Shrink" : "Enlarge"} ${figure.querySelector(".shot-name").textContent}`);
			break;
		}
	}
}

function onCardInput(event) {
	const item = itemOf(event.target);
	if (!item) return;
	if (event.target.dataset.role === "edit") onEditInput(item);
	else if (event.target.dataset.role === "note") onNoteInput(item);
}

function onCardKeydown(event) {
	const role = event.target.dataset?.role;
	const item = role ? itemOf(event.target) : undefined;
	if (!item) return;
	if (role === "edit") {
		if (event.key === "Escape") {
			event.preventDefault();
			if (event.target.value.trim()) saveEdit(item, true);
			else discardEdit(item);
		} else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
			event.preventDefault();
			saveEdit(item, true);
		}
	} else if (role === "note" && event.key === "Escape") {
		event.preventDefault();
		const ui = uiFor(item.id);
		saveNote(item);
		if (!currentNote(item)) ui.noteOpen = false;
		updateCard(item);
		focusCard(item);
	}
}

function onCardFocus(event) {
	const item = itemOf(event.target);
	if (item && state.current !== item.id) setCurrent(item.id);
}

function onCardBlur(event) {
	if (event.target.dataset?.role !== "note") return;
	const item = itemOf(event.target);
	if (item) saveNote(item);
}

function onKey(event) {
	if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
	if (event.key === "Escape" && !els.help.hidden) {
		event.preventDefault();
		toggleHelp(false);
		return;
	}
	if (isTextEntry(event.target)) return;
	if (event.key === "?") {
		event.preventDefault();
		toggleHelp();
		return;
	}
	if (event.key === "/" && state.view === "answered") {
		event.preventDefault();
		$("#answered-query")?.focus();
		return;
	}
	if (state.view !== "part" || !state.part || state.part.error || !state.items.size) return;
	const key = event.key;
	if (key === "/") {
		event.preventDefault();
		els.query.focus();
		els.query.select();
		return;
	}
	if (key === "j" || key === "k") {
		event.preventDefault();
		move(key === "j" ? 1 : -1);
		return;
	}
	if (key === "v") {
		event.preventDefault();
		setCompact(!state.compact);
		return;
	}
	if (event.shiftKey && /^[ARD]$/.test(key)) {
		if (!state.compact || !state.selection.size) return;
		event.preventDefault();
		applySelection(KEY_DECISIONS[key.toLowerCase()]);
		return;
	}
	if (key === "Escape") {
		if (state.selectionAsk) {
			state.selectionAsk = null;
			renderSelectionBar();
			return;
		}
		const item = currentVisible();
		if (state.compact && item && uiFor(item.id).open) {
			setOpen(item, false);
			focusCard(item);
		}
		return;
	}
	const onCard = event.target instanceof Element && event.target.classList.contains("card");
	if (key === "Enter" && !onCard && event.target !== document.body) return;
	if (!["a", "r", "d", "e", "n", "u", "x", "o", "Enter"].includes(key)) return;
	if (!currentVisible()) move(1);
	const item = currentVisible();
	if (!item) return;
	event.preventDefault();
	const ui = uiFor(item.id);
	if (KEY_DECISIONS[key]) {
		decide(item, KEY_DECISIONS[key]);
		// In rows, deciding and moving on folds the opened card back into its row.
		if (state.compact && ui.open) setOpen(item, false);
		move(1);
	} else if (key === "e") openEditor(item);
	else if (key === "n") openNote(item);
	else if (key === "u") {
		clearAnswer(item);
		toast(`Cleared the answer to ${item.id}.`);
	} else if (key === "x") {
		if (state.compact) toggleSelected(item);
	} else if (state.compact) {
		setOpen(item, !ui.open);
		focusCard(item);
	} else if (ui.card.classList.contains("is-collapsed") || ui.expanded) {
		ui.expanded = !ui.expanded;
		updateCard(item);
	}
}

function isTextEntry(target) {
	if (!(target instanceof Element)) return false;
	if (target.closest("textarea, select, [contenteditable]")) return true;
	return target.tagName === "INPUT" && !["checkbox", "radio", "button", "submit", "reset"].includes(target.type);
}

const visibleItems = () => [...state.items.values()].filter((item) => !uiFor(item.id).card.hidden);

function currentVisible() {
	const item = state.current ? state.items.get(state.current) : undefined;
	return item && !uiFor(item.id).card.hidden ? item : undefined;
}

function move(step) {
	const list = visibleItems();
	if (!list.length) return;
	const index = list.findIndex((item) => item.id === state.current);
	const next = index === -1 ? (step > 0 ? 0 : list.length - 1) : Math.min(list.length - 1, Math.max(0, index + step));
	setCurrent(list[next].id, { scroll: true, focus: true });
}

// Shows an item opened from a link or the address: clears filters that hide it, opens its row or
// expands its collapsed card, and scrolls to it, or to its group's heading when it opens the group.
function revealItem(itemId) {
	const item = state.items.get(itemId);
	if (!item) return;
	const ui = uiFor(itemId);
	if (ui.history) ui.history.open = true;
	if (ui.card.hidden) {
		resetFilters();
		toast(`Filters cleared to show ${itemId}.`);
	}
	if (state.compact) ui.open = true;
	else if (ui.card.classList.contains("is-collapsed")) ui.expanded = true;
	updateCard(item);
	setCurrent(itemId, { focus: true });
	const group = ui.card.closest(".group");
	const opensGroup = Boolean(group?.querySelector(".group-title")) && group.querySelector(".card") === ui.card;
	(opensGroup ? group : ui.card).scrollIntoView({ block: "start" });
}

// Clears every filter for this visit; the remembered importance choice stays for the next part.
function resetFilters() {
	Object.assign(state.filter, newFilter(isKeyView() ? "important" : "all"));
	els.query.value = "";
	els.view.value = "all";
	els.unanswered.checked = false;
	els.kind.value = "any";
	els.component.value = "any";
	els.topic.value = "any";
	els.importance.value = state.filter.importance;
	applyFilters();
	renderPartProgress();
}

function setCurrent(itemId, { scroll = false, focus = false } = {}) {
	const ui = state.ui.get(itemId);
	if (!ui?.card) return;
	if (state.current && state.current !== itemId) state.ui.get(state.current)?.card.classList.remove("is-current");
	state.current = itemId;
	ui.card.classList.add("is-current");
	if (focus) ui.card.focus({ preventScroll: true });
	if (scroll) ui.card.scrollIntoView({ block: state.compact && !ui.open ? "nearest" : "start" });
	const href = state.part.virtual ? `#/${state.part.virtual}` : partHref(state.part.id);
	history.replaceState(null, "", `${href}/${encodeURIComponent(itemId)}`);
}

function focusCard(item) {
	uiFor(item.id).card.focus({ preventScroll: true });
}

function focusCurrent() {
	const item = currentVisible();
	if (item) focusCard(item);
}

async function copyValue(button, value) {
	let copied = true;
	try {
		await navigator.clipboard.writeText(value);
	} catch {
		copied = fallbackCopy(value);
	}
	button.classList.add("is-copied");
	setTimeout(() => button.classList.remove("is-copied"), 900);
	toast(copied ? `Copied ${value}` : `Copy failed; select ${value} by hand.`);
}

function fallbackCopy(value) {
	const area = el("textarea", { class: "visually-hidden", "aria-hidden": "true", tabindex: "-1" });
	area.value = value;
	document.body.append(area);
	area.select();
	let copied = false;
	try {
		copied = document.execCommand("copy");
	} catch {}
	area.remove();
	return copied;
}

function showBanner(message, actionLabel) {
	setChildren(
		els.banner,
		el("span", {}, message),
		actionLabel ? el("button", { type: "button", class: "quiet", dataset: { act: "reload" } }, actionLabel) : null,
	);
	els.banner.hidden = false;
}

function hideBanner() {
	els.banner.hidden = true;
}

function toast(message) {
	els.toast.textContent = message;
	els.toast.hidden = false;
	clearTimeout(state.toastTimer);
	state.toastTimer = setTimeout(() => {
		els.toast.hidden = true;
	}, 2400);
}

// ---------------------------------------------------------------------------------------------
// Links: repo paths, pull requests, Linear issues and guide documents

const partHref = (partId) => `#/part/${encodeURIComponent(partId)}`;
const guideHref = (docPath, anchor) => `#/guide/${encodePath(docPath)}${anchor ? `#${encodeURIComponent(anchor)}` : ""}`;
const blobUrl = (repoPath) => `${LOCKDOWN_PATHS.test(repoPath) ? LOCKDOWN_BLOB : PINNED_BLOB}${encodePath(repoPath)}`;
const lineAnchor = (from, to) => (from ? `#L${from}${to ? `-L${to}` : ""}` : "");
const normalizeUrl = (url) => url.replace(OLD_REPO_URL, REPO);

// An item ID links to its card inside the app when some part has that item.
function itemHref(itemId) {
	const partId = state.itemIndex.get(itemId);
	return partId ? `${partHref(partId)}/${encodeURIComponent(itemId)}` : null;
}

function itemLinkHtml(itemId, html = itemId) {
	const href = itemHref(itemId);
	return href ? `<a class="id-link" href="${escapeHtml(href)}">${html}</a>` : html;
}

// Plain text with its item IDs as links, for text that is not Markdown: source labels and refs.
function idNodes(text) {
	const source = String(text);
	const nodes = [];
	let last = 0;
	for (const match of source.matchAll(ITEM_ID)) {
		const href = itemHref(match[0]);
		if (!href) continue;
		if (match.index > last) nodes.push(source.slice(last, match.index));
		nodes.push(el("a", { class: "id-link", href }, match[0]));
		last = match.index + match[0].length;
	}
	if (last < source.length) nodes.push(source.slice(last));
	return nodes;
}

// A link to the item already in the address changes no hash and would do nothing; show the item.
function onItemLinkClick(event) {
	const link = event.target instanceof Element ? event.target.closest("a.id-link") : null;
	if (!link || event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
	if (link.getAttribute("href") !== location.hash) return;
	event.preventDefault();
	route();
}

// The documents the Guide view can show; the server applies the same rule.
function isDocPath(repoPath) {
	if (LOCKDOWN_DOCS.has(repoPath)) return true;
	const segments = repoPath.split("/");
	return (
		(repoPath.startsWith("docs/guide/") || technicalPath(repoPath)) &&
		repoPath.endsWith(".md") &&
		segments.every((segment) => segment && segment !== ".." && !segment.startsWith("."))
	);
}

// A repo path opens in the app when it is a guide document. Otherwise it links to GitHub where it
// exists: on the lockdown branch for the lockdown folders, at the pinned commit for the rest, and
// a folder to its tree. A path known not to exist gets no link (null). Until the server's path
// lists load, or when it cannot list them, every path links as if it existed.
function pathTarget(repoPath, hash = "") {
	if (isDocPath(repoPath)) return { href: guideHref(repoPath, hash && !/^#L\d/.test(hash) ? hash.slice(1) : ""), app: true };
	const lockdown = LOCKDOWN_PATHS.test(repoPath);
	const known = lockdown ? state.repo?.lockdown : state.repo?.pinned;
	if (known && !known.files.has(repoPath)) {
		return known.folders.has(repoPath) ? { href: `${lockdown ? LOCKDOWN_TREE : PINNED_TREE}${encodePath(repoPath)}`, external: true } : null;
	}
	return { href: `${blobUrl(repoPath)}${hash}`, external: true };
}

// origin/<branch>:<path>[:<line>[-<line>]] names a file on another branch; it links to that branch.
const BRANCH_PATH = /^origin\/([\w./-]+):([\w@./-]+?)(?::(\d+)(?:-(\d+))?)?$/;
const branchHref = (match) => `${REPO}/blob/${encodePath(match[1])}/${encodePath(match[2])}${lineAnchor(match[3], match[4])}`;

const REF_TOKEN =
	/https?:\/\/[^\s<>"'()]*[^\s<>"'().,;:!?]|origin\/[\w./-]+:[\w@./-]+|\b[\w.-]+\/[\w.-]+#\d+\b|PR\s*#\d+|\b(?:S27|DIR)-\d+\b|(?:[\w@.-]+\/)+[\w@-]*\.[A-Za-z][\w-]*(?::\d+(?:-\d+)?)?/gi;

// What a source ref links to; a repo path that does not exist gets `href: null` and shows as code.
function refTarget(raw) {
	const ref = raw.trim();
	if (/^https?:\/\/\S+$/i.test(ref)) return { href: normalizeUrl(ref) };
	let match = /^(?:PR\s*#?\s*|#)(\d+)$/i.exec(ref);
	if (match) return { href: `${REPO}/pull/${match[1]}` };
	match = /^([\w.-]+\/[\w.-]+)#(\d+)$/.exec(ref);
	if (match) {
		return { href: SAME_REPO.test(match[1]) ? `${REPO}/issues/${match[2]}` : `https://github.com/${match[1]}/issues/${match[2]}` };
	}
	if (/^(?:S27|DIR)-\d+$/i.test(ref)) return { href: `https://linear.app/935/issue/${ref.toUpperCase()}` };
	match = /^(?:commit\s+)?([0-9a-f]{7,40})$/i.exec(ref);
	if (match && /\d/.test(match[1]) && /[a-f]/i.test(match[1])) return { href: `${REPO}/commit/${match[1]}`, code: true };
	match = BRANCH_PATH.exec(ref);
	if (match) return { href: branchHref(match), code: true };
	// Only "./" or "/" is stripped, so .github/… and .claude/… keep their leading dot.
	match = /^(?:\.?\/)?((?:[\w@.-]+\/)*[\w@.-]+?\/?)(?::(\d+)(?:-(\d+))?)?$/.exec(ref);
	if (match && (match[1].includes("/") || /\.[A-Za-z][\w-]*$/.test(match[1])) && !match[1].split("/").includes("..")) {
		return { href: pathTarget(match[1].replace(/\/$/, ""), lineAnchor(match[2], match[3]))?.href ?? null, code: true };
	}
	return null;
}

// A source ref as nodes: links for URLs, pull requests, Linear issues and repo paths, item IDs as
// links to their cards, and repo paths that do not exist as plain code.
function refNodes(ref) {
	const whole = refTarget(ref);
	if (whole) return [refNode(whole, ref)];
	const nodes = [];
	let last = 0;
	for (const match of ref.matchAll(REF_TOKEN)) {
		const target = refTarget(match[0]);
		if (!target) continue;
		if (match.index > last) nodes.push(...idNodes(ref.slice(last, match.index)));
		nodes.push(refNode(target, match[0]));
		last = match.index + match[0].length;
	}
	if (last < ref.length) nodes.push(...idNodes(ref.slice(last)));
	return nodes;
}

const refNode = (target, text) => (target.href ? externalLink(target.href, text, target.code) : el("code", { class: "code-ref" }, text));

const encodePath = (value) => value.split("/").map(encodeURIComponent).join("/");

function externalLink(href, text, code = false) {
	return el("a", { href, target: "_blank", rel: "noopener noreferrer", class: code ? "code-ref" : null }, text);
}

// ---------------------------------------------------------------------------------------------
// Markdown: the source is HTML-escaped first; only the constructs below produce markup.
// Cards render with a small heading scale; guide documents keep their heading levels, get
// heading anchors, resolve relative links and images against their own folder, and route links
// to other guide documents inside the app.

const FENCE = /^\s{0,3}(`{3,}|~{3,})\s*([\w+#.-]*)\s*$/;
const HEADING = /^\s{0,3}(#{1,6})\s+(.+?)\s*#*\s*$/;
const RULE = /^\s{0,3}([-*_])(?:\s*\1){2,}\s*$/;
const QUOTE = /^\s{0,3}&gt;\s?/;
const LIST_ITEM = /^(\s*)([-*+]|\d{1,9}[.)])\s+(.*)$/;
const DIVIDER_CELL = /^:?-+:?$/;
const LONE_IMAGE = /^!\[[^\]\n]*\]\(\s*(?:[^\s()&]|&amp;)+(?:\s+&quot;[^\n]*?&quot;)?\s*\)$/;

// Card Markdown resolves relative paths from docs/lockdown unless they already start at the repo root.
const CARD_CONTEXT = { doc: false, docPath: null, base: "docs/lockdown", slugs: null, headings: null };
let md = CARD_CONTEXT;

function withContext(context, render) {
	const previous = md;
	md = context;
	try {
		return render();
	} finally {
		md = previous;
	}
}

function escapeHtml(text) {
	return text.replace(/[&<>"']/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char]);
}

function unescapeHtml(text) {
	return text.replace(/&(amp|lt|gt|quot|#39);/g, (_, name) => ({ amp: "&", lt: "<", gt: ">", quot: '"', "#39": "'" })[name]);
}

const textOf = (html) => unescapeHtml(html.replace(/<[^>]*>/g, ""));

function renderMarkdown(source) {
	const text = escapeHtml(String(source ?? "").replace(/\r\n?/g, "\n").replace(/\u0000/g, ""));
	return withContext(CARD_CONTEXT, () => renderBlocks(text.split("\n")));
}

function renderInline(source) {
	return withContext(CARD_CONTEXT, () => inline(escapeHtml(String(source ?? "").replace(/\u0000/g, "")).replace(/\s*\n\s*/g, " ")));
}

// Renders a guide document; a leading H1 becomes the page title.
function renderDocument(source, docPath) {
	const context = { doc: true, docPath, base: docPath.split("/").slice(0, -1).join("/"), slugs: new Map(), headings: [] };
	let text = String(source).replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n").replace(/\u0000/g, "");
	// Front matter is metadata, not prose; a leading thematic break is not front matter.
	const front = /^---\n(?=[\w-]+[ \t]*:)[\s\S]*?\n(?:---|\.\.\.)[ \t]*(?:\n|$)/.exec(text);
	if (front) text = text.slice(front[0].length);
	const lines = text.split("\n");
	const first = lines.findIndex((line) => line.trim());
	const titleMatch = first === -1 ? null : /^\s{0,3}#\s+(.+?)\s*#*\s*$/.exec(lines[first]);
	if (titleMatch) lines.splice(first, 1);
	return withContext(context, () => {
		const titleHtml = titleMatch ? inline(escapeHtml(titleMatch[1])) : "";
		const title = titleMatch ? textOf(titleHtml) : "";
		const titleSlug = titleMatch ? uniqueSlug(slugify(title) || "title") : "";
		const html = renderBlocks(escapeHtml(lines.join("\n")).split("\n"));
		return { title, titleHtml, titleSlug, html, headings: context.headings };
	});
}

function renderBlocks(lines) {
	const html = [];
	let i = 0;
	while (i < lines.length) {
		const line = lines[i];
		if (!line.trim()) {
			i += 1;
			continue;
		}
		const fence = FENCE.exec(line);
		if (fence) {
			const body = [];
			i += 1;
			while (i < lines.length && !lines[i].trim().startsWith(fence[1])) {
				body.push(lines[i]);
				i += 1;
			}
			i += 1;
			const lang = fence[2].toLowerCase();
			html.push(
				lang === "mermaid"
					? `<figure class="md-diagram"><figcaption>Diagram source (Mermaid)</figcaption><pre class="md-mermaid"><code>${body.join("\n")}</code></pre></figure>`
					: `<pre><code>${body.join("\n")}</code></pre>`,
			);
			continue;
		}
		const heading = HEADING.exec(line);
		if (heading) {
			if (md.doc) {
				html.push(docHeading(heading[1].length, heading[2]));
			} else {
				const level = heading[1].length <= 2 ? 4 : 5;
				html.push(`<h${level} class="md-heading">${inline(heading[2])}</h${level}>`);
			}
			i += 1;
			continue;
		}
		if (RULE.test(line)) {
			html.push("<hr>");
			i += 1;
			continue;
		}
		if (QUOTE.test(line)) {
			const quoted = [];
			while (i < lines.length && QUOTE.test(lines[i])) {
				quoted.push(lines[i].replace(QUOTE, ""));
				i += 1;
			}
			html.push(`<blockquote>${renderBlocks(quoted)}</blockquote>`);
			continue;
		}
		if (isTableStart(lines, i)) {
			i = renderTable(lines, i, html);
			continue;
		}
		if (LIST_ITEM.test(line)) {
			i = renderList(lines, i, html);
			continue;
		}
		const paragraph = [];
		while (i < lines.length && lines[i].trim() && !(paragraph.length && startsBlock(lines, i))) {
			paragraph.push(lines[i].trim());
			i += 1;
		}
		const joined = paragraph.join(" ");
		if (LONE_IMAGE.test(joined)) {
			const alt = /^!\[([^\]\n]*)\]/.exec(joined)[1];
			html.push(`<figure class="md-figure">${inline(joined)}${alt ? `<figcaption>${inline(alt)}</figcaption>` : ""}</figure>`);
		} else {
			html.push(`<p>${inline(joined)}</p>`);
		}
	}
	return html.join("");
}

function docHeading(level, source) {
	const inner = inline(source);
	const text = textOf(inner);
	const slug = uniqueSlug(slugify(text) || "section");
	const shown = Math.min(Math.max(level, 2), 4);
	md.headings.push({ level: Math.max(level, 2), slug, text });
	return `<h${shown} class="doc-h" id="doc-${slug}" tabindex="-1">${inner}<a class="doc-anchor" href="${guideHref(md.docPath, slug)}" aria-label="Link to ${escapeHtml(text)}">§</a></h${shown}>`;
}

// GitHub's heading anchors: lower case, punctuation dropped, each space a hyphen.
function slugify(text) {
	return text.trim().toLowerCase().replace(/[^\p{L}\p{M}\p{N}\p{Pc}\- ]/gu, "").replace(/ /g, "-");
}

function uniqueSlug(slug) {
	const count = md.slugs.get(slug) ?? 0;
	md.slugs.set(slug, count + 1);
	return count ? `${slug}-${count}` : slug;
}

function startsBlock(lines, i) {
	const line = lines[i];
	return FENCE.test(line) || HEADING.test(line) || RULE.test(line) || QUOTE.test(line) || LIST_ITEM.test(line) || isTableStart(lines, i);
}

function splitRow(line) {
	let row = line.trim();
	if (row.startsWith("|")) row = row.slice(1);
	if (row.endsWith("|") && !row.endsWith("\\|")) row = row.slice(0, -1);
	const cells = [];
	let cell = "";
	let code = false;
	for (let k = 0; k < row.length; k += 1) {
		const char = row[k];
		if (char === "\\" && row[k + 1] === "|") {
			cell += "|";
			k += 1;
			continue;
		}
		if (char === "`") code = !code;
		if (char === "|" && !code) {
			cells.push(cell.trim());
			cell = "";
			continue;
		}
		cell += char;
	}
	cells.push(cell.trim());
	return cells;
}

function isTableStart(lines, i) {
	if (!lines[i].includes("|") || i + 1 >= lines.length || !lines[i + 1].includes("-")) return false;
	const divider = splitRow(lines[i + 1]);
	return divider.every((cell) => DIVIDER_CELL.test(cell)) && splitRow(lines[i]).length === divider.length;
}

function renderTable(lines, start, html) {
	const header = splitRow(lines[start]);
	const align = splitRow(lines[start + 1]).map((cell) =>
		cell.startsWith(":") && cell.endsWith(":") ? ' class="al-c"' : cell.endsWith(":") ? ' class="al-r"' : "",
	);
	const rows = [];
	let i = start + 2;
	while (i < lines.length && lines[i].trim() && lines[i].includes("|")) {
		const cells = splitRow(lines[i]);
		rows.push(`<tr>${header.map((_, k) => `<td${align[k]}>${inline(cells[k] ?? "")}</td>`).join("")}</tr>`);
		i += 1;
	}
	const head = `<tr>${header.map((cell, k) => `<th${align[k]}>${inline(cell)}</th>`).join("")}</tr>`;
	html.push(`<div class="table-wrap"><table><thead>${head}</thead><tbody>${rows.join("")}</tbody></table></div>`);
	return i;
}

function renderList(lines, start, html) {
	const stack = [];
	let out = "";
	let i = start;
	while (i < lines.length) {
		const line = lines[i];
		const match = LIST_ITEM.exec(line);
		if (match) {
			const indent = match[1].replace(/\t/g, "    ").length;
			const tag = /\d/.test(match[2]) ? "ol" : "ul";
			let top = stack.at(-1);
			if (!top || indent >= top.indent + 2) {
				const first = tag === "ol" ? Number.parseInt(match[2], 10) : 1;
				out += `<${tag}${first !== 1 ? ` start="${first}"` : ""}><li>`;
				stack.push({ indent, tag });
			} else {
				while (stack.length > 1 && indent < top.indent) {
					out += `</li></${top.tag}>`;
					stack.pop();
					top = stack.at(-1);
				}
				if (top.tag !== tag) {
					out += `</li></${top.tag}><${tag}><li>`;
					top.tag = tag;
				} else {
					out += "</li><li>";
				}
			}
			out += inline(match[3]);
			i += 1;
			continue;
		}
		if (!line.trim()) {
			const next = lines[i + 1];
			if (next !== undefined && (LIST_ITEM.test(next) || /^\s{2,}\S/.test(next))) {
				i += 1;
				continue;
			}
			break;
		}
		if (/^\s{2,}\S/.test(line) || !startsBlock(lines, i)) {
			out += ` ${inline(line.trim())}`;
			i += 1;
			continue;
		}
		break;
	}
	while (stack.length) out += `</li></${stack.pop().tag}>`;
	html.push(out);
	return i;
}

// Inline markup over escaped text. Code spans, images and links become placeholders first so
// emphasis never reaches into them. Only http(s) links leave the app; relative paths resolve to
// guide documents (in the app) or to the repository on GitHub; anything else stays plain text.
function inline(text) {
	const tokens = [];
	const keep = (html) => `\u0000${tokens.push(html) - 1}\u0000`;
	// Placeholders inside an attribute value would smuggle markup into it; use their text instead.
	const flat = (value) => value.replace(/\u0000(\d+)\u0000/g, (_, index) => escapeHtml(textOf(tokens[Number(index)] ?? "")));
	let out = text.replace(/(`+)([^`]|[^`][\s\S]*?[^`])\1(?!`)/g, (_, _ticks, code) => keep(codeSpan(code.trim())));
	out = out.replace(/!\[([^\]\n]*)\]\(\s*((?:[^\s()&]|&amp;)+)(?:\s+&quot;[^\n]*?&quot;)?\s*\)/g, (_, alt, target) =>
		keep(image(flat(alt), target)),
	);
	out = out.replace(/\[([^\]\n]+)\]\(\s*((?:[^\s()&]|&amp;)+)(?:\s+&quot;[^\n]*?&quot;)?\s*\)/g, (_, label, target) => {
		const link = resolveLink(target);
		if (link?.sourcePath) {
			const path = keep(`<code>${escapeHtml(link.sourcePath)}</code>`);
			return flat(label) === escapeHtml(link.sourcePath) ? path : `${label} (${path})`;
		}
		return link ? keep(anchorTag(link, emphasis(label))) : label;
	});
	out = out.replace(/&lt;(https?:\/\/(?:[^\s&]|&amp;)+?)&gt;/gi, (_, url) => keep(anchor(url, url)));
	out = out.replace(
		/(^|[\s(])(https?:\/\/(?:[^\s<>"'&\u0000]|&amp;)*[^\s<>"'&\u0000.,;:!?)\]])/gi,
		(_, before, url) => before + keep(anchor(url, url)),
	);
	out = out.replace(/\bPR\s?#(\d{1,6})\b/g, (whole, number) => keep(anchorTag({ href: `${REPO}/pull/${number}`, external: true }, whole)));
	out = out.replace(/\b(?:S27|DIR)-\d{1,6}\b/g, (whole) =>
		keep(anchorTag({ href: `https://linear.app/935/issue/${whole}`, external: true }, whole)),
	);
	out = out.replace(ITEM_ID, (whole) => (state.itemIndex.has(whole) ? keep(itemLinkHtml(whole)) : whole));
	out = emphasis(out);
	for (let pass = 0; pass < 4 && out.includes("\u0000"); pass += 1) {
		out = out.replace(/\u0000(\d+)\u0000/g, (_, index) => tokens[Number(index)] ?? "");
	}
	return out;
}

function emphasis(text) {
	return text
		.replace(/\*\*(?=\S)([^*]*?\S)\*\*/g, "<strong>$1</strong>")
		.replace(/(^|[^\w])__(?=\S)([^_]*?\S)__(?!\w)/g, "$1<strong>$2</strong>")
		.replace(/(^|[^\w*])\*(?=[^\s*])([^*\n]*?[^\s*])\*(?![\w*])/g, "$1<em>$2</em>")
		.replace(/(^|[^\w])_(?=[^\s_])([^_\n]*?[^\s_])_(?!\w)/g, "$1<em>$2</em>")
		.replace(/~~(?=\S)([^~]*?\S)~~/g, "<del>$1</del>");
}

function anchor(href, label) {
	return `<a href="${normalizeUrl(href)}" target="_blank" rel="noopener noreferrer">${label}</a>`;
}

function anchorTag(target, html, extraClass = "") {
	const className = [target.app ? "app-link" : "", extraClass].filter(Boolean).join(" ");
	// Guide links move within the Guide view; from a card they open a new tab so the review keeps its place.
	const newTab = target.external || !md.doc;
	return `<a href="${escapeHtml(target.href)}"${className ? ` class="${className}"` : ""}${newTab ? ' target="_blank" rel="noopener noreferrer"' : ""}>${html}</a>`;
}

// A code span that names an item links to its card, and one that names a repo path links to it
// where it exists: `crates/richview-core/src/lib.rs:1-9`, `.github/workflows/ci.yml`, or
// `origin/<branch>:<path>` on that branch. Anything else stays plain code.
function codeSpan(code) {
	const html = `<code>${code}</code>`;
	const text = unescapeHtml(code);
	if (state.itemIndex.has(text)) return itemLinkHtml(text, html);
	const branch = BRANCH_PATH.exec(text);
	if (branch) return anchorTag({ href: branchHref(branch), external: true }, html, "code-link");
	const match = /^(?:\.\/)?([\w@.+-]+(?:\/[\w@.+-]+)*\/?)(?::(\d+)(?:-(\d+))?)?$/.exec(text);
	if (!match) return html;
	const repoPath = match[1].replace(/\/$/, "");
	if (repoPath.split("/").includes("..")) return html;
	// With the server's path lists any path-like name may link; without them only names under a
	// top-level entry of the repository do.
	const known = state.repo && (LOCKDOWN_PATHS.test(repoPath) ? state.repo.lockdown : state.repo.pinned);
	const pathLike = known
		? match[1].includes("/") || /\.[A-Za-z][\w-]*$/.test(repoPath) || REPO_FILES.has(repoPath)
		: REPO_DIRS.test(match[1]) || REPO_FILES.has(repoPath);
	const target = pathLike ? pathTarget(repoPath, lineAnchor(match[2], match[3])) : null;
	return target ? anchorTag(target, html, "code-link") : html;
}

function safeDecode(value) {
	try {
		return decodeURIComponent(value);
	} catch {
		return value;
	}
}

// Resolves a relative link target to a repo path, or null when it leaves the repository.
function resolveRepoPath(target) {
	let rest = target;
	let base;
	if (rest.startsWith("/")) {
		base = "";
		rest = rest.replace(/^\/+/, "");
	} else if (md.doc) {
		base = md.base;
	} else {
		base = REPO_DIRS.test(rest) || REPO_FILES.has(rest.replace(/\/$/, "")) ? "" : md.base;
	}
	const out = base ? base.split("/") : [];
	for (const segment of rest.split("/")) {
		if (!segment || segment === ".") continue;
		if (segment === "..") {
			if (!out.length) return null;
			out.pop();
		} else {
			out.push(segment);
		}
	}
	return out.length ? out.join("/") : null;
}

function resolveLink(raw) {
	const target = unescapeHtml(raw).trim();
	if (/^https?:\/\//i.test(target)) return /^https?:\/\/\S+$/i.test(target) ? { href: normalizeUrl(target), external: true } : null;
	if (/^[a-z][a-z\d+.-]*:/i.test(target) || target.startsWith("//")) return null;
	const hashAt = target.indexOf("#");
	const pathPart = (hashAt === -1 ? target : target.slice(0, hashAt)).replace(/\?.*$/, "");
	const fragment = hashAt === -1 ? "" : safeDecode(target.slice(hashAt + 1));
	if (!pathPart) return md.doc && fragment ? { href: guideHref(md.docPath, fragment), app: true } : null;
	const repoPath = resolveRepoPath(safeDecode(pathPart));
	if (!repoPath) return null;
	if (isDocPath(repoPath)) return { href: guideHref(repoPath, fragment), app: true };
	if (technicalPath(md.docPath)) return { sourcePath: repoPath + (fragment ? `#${fragment}` : "") };
	const hash = /^L\d+(?:-L\d+)?$/.test(fragment) ? `#${fragment}` : fragment ? `#${encodeURIComponent(fragment)}` : "";
	// A path that does not exist leaves the label as plain text.
	return pathTarget(repoPath, hash);
}

// Images resolve like links; the app serves only docs/lockdown/screenshots and docs/guide, and its
// content security policy blocks remote images, so anything else becomes a link instead.
function image(alt, raw) {
	const target = unescapeHtml(raw).trim();
	if (/^https?:\/\/\S+$/i.test(target)) {
		return anchorTag({ href: normalizeUrl(target), external: true }, `Image: ${alt || escapeHtml(target)}`, "md-image-link");
	}
	if (/^[a-z][a-z\d+.-]*:/i.test(target) || target.startsWith("//")) return alt;
	const repoPath = resolveRepoPath(safeDecode(target.replace(/[?#].*$/, "")));
	if (!repoPath) return alt;
	if (SERVED_IMAGE.test(repoPath) && !repoPath.split("/").includes("..")) {
		const src = `/files/${encodePath(repoPath)}`;
		return `<a class="md-image" href="${src}" target="_blank" rel="noopener"><img src="${src}" alt="${alt}" loading="lazy" decoding="async" data-path="${escapeHtml(repoPath)}"></a>`;
	}
	const label = `Image: ${alt || escapeHtml(repoPath)}`;
	const link = pathTarget(repoPath);
	return link ? anchorTag(link, label, "md-image-link") : label;
}

// ---------------------------------------------------------------------------------------------
// Small helpers

// Titles are plain text, except that `code` spans render as code.
const titleNodes = (title) => String(title).split(/`([^`\n]+)`/).map((part, index) => (index % 2 ? el("code", {}, part) : part));

function el(tag, props = {}, ...children) {
	const node = document.createElement(tag);
	for (const [key, value] of Object.entries(props)) {
		if (value === undefined || value === null || value === false) continue;
		if (key === "class") node.className = value;
		else if (key === "html") node.innerHTML = value;
		else if (key === "dataset") Object.assign(node.dataset, value);
		else node.setAttribute(key, value === true ? "" : String(value));
	}
	for (const child of children.flat()) if (child !== null && child !== undefined && child !== false && child !== "") node.append(child);
	return node;
}

// Replaces a node's children, skipping empty entries (replaceChildren would print "null").
function setChildren(node, ...children) {
	node.replaceChildren(...children.flat().filter((child) => child !== null && child !== undefined && child !== false && child !== ""));
}

function bar(value, max, className) {
	const fill = el("span", { class: "bar-fill" });
	fill.style.width = `${max ? Math.round((value / max) * 100) : 0}%`;
	return el(
		"span",
		{
			class: `bar ${className}`,
			role: "progressbar",
			"aria-valuemin": "0",
			"aria-valuemax": String(max),
			"aria-valuenow": String(value),
			"aria-label": `${value} of ${max} answered`,
		},
		fill,
	);
}

function tallyAnswers(items) {
	const decisions = { accept: 0, edit: 0, reject: 0, defer: 0 };
	let answered = 0;
	for (const item of items) {
		if (!item.answer) continue;
		answered += 1;
		decisions[item.answer.decision] += 1;
	}
	return { answered, decisions };
}

function decisionText(decisions) {
	return `${decisions.accept} accepted · ${decisions.edit} edited · ${decisions.reject} rejected · ${decisions.defer} deferred`;
}

function autoGrow(area) {
	area.style.height = "auto";
	area.style.height = `${area.scrollHeight + 2}px`;
}

function formatTime(iso) {
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return "";
	const time = date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
	return date.toDateString() === new Date().toDateString()
		? time
		: `${date.toLocaleDateString([], { day: "numeric", month: "short" })} ${time}`;
}

// The item's importance (key, important or detail), or "" when it is unrated; a server started
// before the field existed passes it through raw.
function importanceOf(item) {
	const value = typeof item.importance === "string" ? item.importance.trim().toLowerCase() : "";
	return IMPORTANCE.includes(value) ? value : "";
}

const importanceRank = (item) => {
	const index = IMPORTANCE.indexOf(importanceOf(item));
	return index === -1 ? IMPORTANCE.length : index;
};

// Whether an item passes an importance filter: all, key, or important (key and important).
function importanceIn(item, filter) {
	if (filter === "key") return importanceOf(item) === "key";
	if (filter === "important") return importanceRank(item) <= 1;
	return true;
}

// The items grouped as groupOf groups them, groups in order of first appearance, and within each
// group the most important first, otherwise in their own order.
function byImportance(items, groupOf) {
	const groups = new Map();
	for (const item of items) {
		const key = groupOf(item);
		if (!groups.has(key)) groups.set(key, []);
		groups.get(key).push(item);
	}
	return [...groups.values()].flatMap((members) =>
		members
			.map((item, index) => [item, index])
			.sort((a, b) => importanceRank(a[0]) - importanceRank(b[0]) || a[1] - b[1])
			.map(([item]) => item),
	);
}

const isKeyView = () => state.partId === KEY_VIEW;
const groupOf = (item) => item.viewGroup ?? (item.group || "");

// The item's optional component ID; a server started before the field existed passes it through raw.
const componentOf = (item) => (typeof item.component === "string" ? item.component.trim() : "");
const sentence = (text) => (/[.!?]$/.test(text) ? text : `${text}.`);
const plural = (count, noun) => `${count} ${noun}${count === 1 ? "" : "s"}`;
const capitalize = (text) => (text ? text[0].toUpperCase() + text.slice(1) : text);
const slug = (text) => String(text).toLowerCase().replace(/[^a-z0-9]+/g, "-");
const shorten = (text, max) => (text.length > max ? `${text.slice(0, max - 1)}…` : text);
const orderLabel = (order) => (Number.isSafeInteger(order) && order >= 0 && order < 1000 ? String(order).padStart(2, "0") : "");
// "04a" from 04a-features-charts; otherwise the part's order.
const partNumber = (part) => /^(\d{1,3}[a-z]?)(?=[-_.]|$)/i.exec(part.id)?.[1].toLowerCase() ?? orderLabel(part.order);
const docDepth = (docPath) => (docPath.startsWith("docs/guide/") ? docPath.split("/").length - 3 : 0);

function debounce(fn, ms) {
	let timer = 0;
	return (...args) => {
		clearTimeout(timer);
		timer = setTimeout(() => fn(...args), ms);
	};
}

function readPref(key) {
	try {
		return localStorage.getItem(`richview-lockdown.${key}`);
	} catch {
		return null;
	}
}

// A stored preference when it is one of the allowed values, otherwise the fallback.
function prefIn(key, allowed, fallback) {
	const value = readPref(key);
	return allowed.includes(value) ? value : fallback;
}

function writePref(key, value) {
	try {
		localStorage.setItem(`richview-lockdown.${key}`, value);
	} catch {}
}

// Last, so every module-level constant above is initialised before the first route runs.
init();
