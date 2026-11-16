// Browser tests for reviewing by importance: the importance and topic filters of a part, the
// cross-part Key decisions view (#/key) saving through the same path as a part, answered items
// reading as done, and the answer help. Uses the repository's Playwright (Chromium) and skips when
// it is not installed.
// Run: node --test tools/lockdown-app/test/ui-review.test.mjs
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { after, before, test } from "node:test";

const SERVER = path.join(import.meta.dirname, "..", "server.mjs");
let chromium;
try {
	({ chromium } = await import(path.join(import.meta.dirname, "..", "..", "..", "node_modules", "playwright", "index.mjs")));
} catch {
	chromium = null;
}
const skip = chromium ? false : "Playwright is not installed (npm ci in the repository root)";

let root;
let server;
let port;
let browser;

const item = (id, importance, group, extra = {}) => ({
	id,
	kind: "decision",
	group,
	title: `Title ${id}`,
	question: "Keep it?",
	proposed: `Proposal for ${id}.`,
	confidence: "observed",
	sources: [{ label: "Code", ref: "README.md:1" }],
	...(importance ? { importance } : {}),
	...extra,
});

const OPTIONS = {
	options: [
		{ label: "Keep the noun", detail: "Presentation layer." },
		{ label: "Change the noun", detail: "Report product." },
	],
	recommendation: "Keep the noun: it is what the code does.",
};

const PARTS = {
	"01-intent": {
		order: 1,
		title: "Purpose",
		items: [
			item("INT-01", "detail", "Definition"),
			item("INT-02", "key", "Definition", OPTIONS),
			item("INT-03", "important", "Audience"),
			item("INT-04", null, "Audience"),
		],
	},
	"02-delivery": { order: 2, title: "Delivery", items: [item("DEL-01", "key", "Gates"), item("DEL-02", "detail", "Gates")] },
	"03-architecture": { order: 3, title: "Architecture", items: [item("ARC-01", "key", "Components")] },
	"08-open-work": {
		order: 8,
		title: "Open work",
		items: [
			item("OPN-50", "key", "Cross-part contradictions"),
			item("OPN-51", "important", "Cross-part contradictions"),
			item("OPN-70", "key", "Other open work"),
			item("OPN-71", "detail", "Other open work"),
		],
	},
};

async function readAnswers(partId) {
	try {
		return JSON.parse(await readFile(path.join(root, "docs", "lockdown", "answers", `${partId}.json`), "utf8")).answers;
	} catch (error) {
		if (error.code === "ENOENT") return {};
		throw error;
	}
}

function startServer(dir) {
	return new Promise((resolve, reject) => {
		const child = spawn(process.execPath, [SERVER, "--port", "0", "--root", dir], { stdio: ["ignore", "pipe", "pipe"] });
		let output = "";
		child.stdout.on("data", (chunk) => {
			output += chunk;
			const match = /http:\/\/127\.0\.0\.1:(\d+)\//.exec(output);
			if (match) resolve({ child, port: Number(match[1]) });
		});
		child.stderr.on("data", (chunk) => {
			output += chunk;
		});
		child.on("exit", (code) => reject(new Error(`The server exited (${code}): ${output}`)));
	});
}

async function until(check, label, timeout = 12_000) {
	const end = Date.now() + timeout;
	for (;;) {
		const value = await check();
		if (value) return value;
		if (Date.now() > end) throw new Error(`Timed out waiting for ${label}`);
		await new Promise((resolve) => setTimeout(resolve, 200));
	}
}

const card = (page, id) => page.locator(`article[data-item="${id}"]`);
const shownIds = (page) => page.locator("#cards article.card:not([hidden])").evaluateAll((cards) => cards.map((entry) => entry.dataset.item));

async function open(page, hash, firstId) {
	await page.goto(`http://127.0.0.1:${port}/${hash}`);
	await card(page, firstId).waitFor();
}

before(async () => {
	if (skip) return;
	root = await mkdtemp(path.join(os.tmpdir(), "lockdown-ui-review-"));
	await mkdir(path.join(root, "docs", "lockdown", "parts"), { recursive: true });
	for (const [id, part] of Object.entries(PARTS)) {
		const body = { id, status: "ready", summary: "Test part.", ...part };
		await writeFile(path.join(root, "docs", "lockdown", "parts", `${id}.json`), JSON.stringify(body));
	}
	({ child: server, port } = await startServer(root));
	browser = await chromium.launch();
});

after(async () => {
	await browser?.close();
	server?.kill();
	if (root) await rm(root, { recursive: true, force: true });
});

test("the importance filter narrows a part, sorts key first, is remembered across parts and limits Select all shown", { skip }, async () => {
	const context = await browser.newContext();
	const page = await context.newPage();
	await open(page, "#/part/01-intent", "INT-01");
	// Within a group the key card comes first; an unrated card shows no badge.
	assert.deepEqual(await shownIds(page), ["INT-02", "INT-01", "INT-03", "INT-04"]);
	assert.equal(await card(page, "INT-02").locator(".card-margin .importance-key").textContent(), "Key");
	assert.equal(await card(page, "INT-01").locator(".card-margin .importance").textContent(), "Detail");
	assert.equal(await card(page, "INT-04").locator(".importance").count(), 0);
	assert.match(await page.locator("#part-progress").textContent(), /0 of 4 answered\s*key 0 of 1/);

	await page.selectOption("#filter-importance", "key");
	assert.deepEqual(await shownIds(page), ["INT-02"]);
	assert.equal(await page.locator("#showing").textContent(), "Showing 1 of 4");
	await page.selectOption("#filter-importance", "important");
	assert.deepEqual(await shownIds(page), ["INT-02", "INT-03"]);
	assert.equal(await page.evaluate(() => localStorage.getItem("richview-lockdown.importance")), "important");

	// Another part opens with the remembered choice; Select all shown and the bulk decision only
	// reach what it shows.
	await open(page, "#/part/02-delivery", "DEL-01");
	assert.equal(await page.locator("#filter-importance").inputValue(), "important");
	assert.deepEqual(await shownIds(page), ["DEL-01"]);
	await page.locator("[data-layout='rows']").click();
	assert.equal(await page.locator("#select-all-label").textContent(), "Select all 1 shown");
	await page.locator("#select-all").check();
	await page.locator("#selection-bar [data-act='sel-defer']").click();
	await until(async () => (await readAnswers("02-delivery"))["DEL-01"]?.decision === "defer", "the bulk defer");
	assert.equal((await readAnswers("02-delivery"))["DEL-02"], undefined);
	await page.selectOption("#filter-importance", "all");
	assert.deepEqual(await shownIds(page), ["DEL-01", "DEL-02"]);
	await context.close();
});

test("the topic filter and Unanswered only combine with importance; answered rows read as done", { skip }, async () => {
	const context = await browser.newContext();
	const page = await context.newPage();
	await open(page, "#/part/01-intent", "INT-01");
	await page.locator("[data-layout='rows']").click();
	assert.deepEqual(await page.locator("#filter-topic option").allTextContents(), ["Any topic", "Definition (2)", "Audience (2)"]);
	await page.selectOption("#filter-topic", "Audience");
	assert.deepEqual(await shownIds(page), ["INT-03", "INT-04"]);
	await page.selectOption("#filter-importance", "important");
	assert.deepEqual(await shownIds(page), ["INT-03"]);
	await page.selectOption("#filter-importance", "all");

	// An answered row says what it holds; an open one says Open.
	await card(page, "INT-03").locator(".row-actions [data-act='accept']").click();
	await until(async () => (await readAnswers("01-intent"))["INT-03"]?.decision === "accept", "the accept");
	const badge = card(page, "INT-03").locator(".row-meta .decision-state");
	assert.equal(await badge.textContent(), "Accepted");
	assert.match(await badge.getAttribute("class"), /is-accept/);
	assert.equal(await card(page, "INT-04").locator(".row-meta .decision-state").textContent(), "Open");

	await page.locator("#filter-unanswered").check();
	assert.deepEqual(await shownIds(page), ["INT-04"]);
	assert.equal(await page.locator("#filter-view").inputValue(), "unanswered");
	await page.selectOption("#filter-topic", "any");
	assert.deepEqual(await shownIds(page), ["INT-02", "INT-01", "INT-04"]);
	await page.locator("#filter-unanswered").uncheck();
	assert.equal(await page.locator("#filter-view").inputValue(), "all");
	await context.close();
});

test("#/key lists every key card in the start page's order and saves through the part's own path", { skip }, async () => {
	const context = await browser.newContext();
	const page = await context.newPage();
	await page.goto(`http://127.0.0.1:${port}/#/`);
	// The tests share one server: DEL-01 was deferred and INT-03 accepted above.
	const startLine = page.locator(".start-key-line");
	await until(async () => /Key decisions\s*1 of 5 answered/.test(await startLine.textContent()), "the key progress on the start page");
	await page.locator("#rail-key").click();
	await card(page, "OPN-50").waitFor();
	assert.equal(await page.locator("#rail-key").getAttribute("aria-current"), "page");
	assert.deepEqual(await page.locator(".group-title").evaluateAll((titles) => titles.map((title) => title.firstChild.textContent)), [
		"Part 08 · Cross-part contradictions",
		"Part 01 · Purpose",
		"Part 02 · Delivery",
		"Part 03 · Architecture",
		"Part 08 · Open work",
	]);
	assert.deepEqual(await shownIds(page), ["OPN-50", "INT-02", "DEL-01", "ARC-01", "OPN-70"]);
	assert.match(await page.locator("#part-progress").textContent(), /1 of 5 key decisions answered/);

	// Choosing an option on a card opened in place saves to that card's part.
	await card(page, "INT-02").locator(".row-title").click();
	await card(page, "INT-02").locator("[data-act='choose'][data-option='1']").click();
	await until(async () => (await readAnswers("01-intent"))["INT-02"]?.decision === "edit", "the choice saved to part 01");
	assert.match((await readAnswers("01-intent"))["INT-02"].text, /^Chosen option: Change the noun\./);
	assert.equal(await card(page, "INT-02").locator(".card-status .decision-state").textContent(), "Chosen: Change the noun");
	await until(async () => /Saved as your choice/.test(await card(page, "INT-02").locator(".card-status .save-state").textContent()), "Saved as your choice");
	assert.match(await page.locator(".group[data-group='Part 01 · Purpose'] .group-count").textContent(), /1 of 1 answered/);

	// A decision that fails is journaled under its own part and retried, as in a part.
	await page.route("**/api/answers/**", (route) => route.abort("connectionrefused"));
	await card(page, "OPN-50").locator(".row-actions [data-act='reject']").click();
	await until(async () => (await card(page, "OPN-50").textContent()).includes("Not saved"), "Not saved");
	const journal = JSON.parse(await page.evaluate(() => localStorage.getItem("lockdown.journal.v1")));
	assert.equal(journal.entries["08-open-work/OPN-50"].intent.decision, "reject");
	await page.unroute("**/api/answers/**");
	// The network is back: say so, as the browser does, rather than wait for the 5 s poll, whose
	// refresh can take longer than the deadline on a loaded host (S27-714). The app skips a retry
	// while one is reading the part or saving the card, so the event is sent until one lands. The
	// poll's own retry stays covered by ui-journal.test.mjs.
	await until(async () => {
		await page.evaluate(() => window.dispatchEvent(new Event("online")));
		return (await readAnswers("08-open-work"))["OPN-50"]?.decision === "reject";
	}, "the retried reject");
	const log = (await readFile(path.join(root, "docs", "lockdown", "answers", "log.jsonl"), "utf8")).trim().split("\n").map((line) => JSON.parse(line));
	assert.ok(log.some((line) => line.partId === "08-open-work" && line.itemId === "OPN-50" && line.decision === "reject"));
	await until(async () => /3 of 5 key decisions answered/.test(await page.locator("#part-progress").textContent()), "the progress");

	// Unanswered only, then widening to key and important cards.
	await page.locator("#filter-unanswered").check();
	assert.deepEqual(await shownIds(page), ["ARC-01", "OPN-70"]);
	await page.selectOption("#filter-importance", "important");
	assert.deepEqual(await shownIds(page), ["OPN-51", "ARC-01", "OPN-70"]);
	assert.match(await page.locator("#part-progress").textContent(), /4 of 7 key and important cards answered/);
	assert.equal(await page.evaluate(() => localStorage.getItem("richview-lockdown.importance.key")), "important");
	// The part filter keeps its own remembered choice.
	assert.equal(await page.evaluate(() => localStorage.getItem("richview-lockdown.importance")), null);

	// The part shows the answer given in the Key decisions view.
	await open(page, "#/part/01-intent", "INT-02");
	assert.equal(await card(page, "INT-02").locator(".card-status .decision-state").textContent(), "Chosen: Change the noun");
	assert.equal(await page.evaluate(() => localStorage.getItem("lockdown.journal.v1")), null);
	// The address of a key card opens it in the Key decisions view.
	await page.goto(`http://127.0.0.1:${port}/#/key/ARC-01`);
	await until(async () => (await card(page, "ARC-01").getAttribute("class")).includes("is-open"), "ARC-01 opened in place");
	await context.close();
});

test("answering explains itself: tooltips on every answer button, the options line and the help panel", { skip }, async () => {
	const context = await browser.newContext();
	const page = await context.newPage();
	await open(page, "#/part/03-architecture", "ARC-01");
	const arc = card(page, "ARC-01");
	const help = async (selector) => {
		const button = arc.locator(selector).first();
		return [await button.getAttribute("title"), await button.getAttribute("aria-description")];
	};
	for (const [selector, text] of [
		[".actions [data-act='accept']", /Keep the proposal as written/],
		[".actions [data-act='edit']", /replaces the proposal, so no Accept is needed/],
		[".actions [data-act='reject']", /rewrite does not build or follow it/],
		[".actions [data-act='defer']", /Decide later/],
		[".row-actions [data-act='accept']", /Keep the proposal as written/],
		["[data-act='note']", /not an answer/],
		["[data-act='clear']", /Remove your decision/],
	]) {
		const [title, description] = await help(selector);
		assert.match(title, text, selector);
		assert.equal(description, title, selector);
	}
	await open(page, "#/part/01-intent", "INT-02");
	const int = card(page, "INT-02");
	assert.match(await int.locator(".options-hint").textContent(), /Choose an option, or Accept takes the recommended one\. Choosing is your whole answer\./);
	assert.match(await int.locator("[data-act='choose']").first().getAttribute("title"), /that is your full answer/);
	// INT-02 holds a choice (above): Accept says it would replace it.
	assert.match(await int.locator(".actions [data-act='accept']").getAttribute("title"), /Replaces your choice with the recommended option/);

	// The help opens from the header and with ?, and Esc closes it.
	await page.locator("#help-toggle").click();
	assert.equal(await page.locator("#help").isVisible(), true);
	assert.match(await page.locator("#help").textContent(), /Note.*A comment for the agents, not an answer/s);
	await page.keyboard.press("Escape");
	assert.equal(await page.locator("#help").isHidden(), true);
	await page.locator("body").press("?");
	assert.equal(await page.locator("#help").isVisible(), true);
	assert.equal(await page.locator("#help-toggle").getAttribute("aria-expanded"), "true");
	await page.keyboard.press("?");
	assert.equal(await page.locator("#help").isHidden(), true);
	await context.close();
});
