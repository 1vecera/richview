// Browser tests for answers that must not be lost: failed saves, reloads, notes before a decision,
// conflicts with a newer saved answer and the close-page warning. Uses the repository's Playwright
// (Chromium) and skips when it is not installed.
// Run: node --test tools/lockdown-app/test/ui-journal.test.mts
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import type { ChildProcessByStdio } from "node:child_process";
import type { Readable } from "node:stream";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { after, before, test } from "node:test";
import type { Browser, BrowserType, Page } from "playwright";
import type { SavedAnswer } from "../types.mts";

const SERVER = path.join(import.meta.dirname, "..", "server.mjs");
let chromium: BrowserType | null;
try {
	({ chromium } = await import(path.join(import.meta.dirname, "..", "..", "..", "node_modules", "playwright", "index.mjs")));
} catch {
	chromium = null;
}
const skip = chromium ? false : "Playwright is not installed (npm ci in the repository root)";

type TestServer = ChildProcessByStdio<null, Readable, Readable>;

let root: string;
let server: TestServer;
let port: number;
let browser: Browser;

const PART = "01-intent";
const item = (id: string) => ({
	id,
	kind: "decision",
	title: `Title ${id}`,
	question: "Keep it?",
	proposed: `Proposal for ${id}.`,
	confidence: "observed",
	sources: [{ label: "Code", ref: "README.md:1" }],
});

async function readAnswers(): Promise<Record<string, SavedAnswer>> {
	try {
		return JSON.parse(await readFile(path.join(root, "docs", "lockdown", "answers", `${PART}.json`), "utf8")).answers;
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return {};
		throw error;
	}
}

function startServer(dir: string): Promise<{ child: TestServer; port: number }> {
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

async function until<T>(check: () => T | Promise<T>, label: string, timeout = 12_000) {
	const end = Date.now() + timeout;
	for (;;) {
		const value = await check();
		if (value) return value;
		if (Date.now() > end) throw new Error(`Timed out waiting for ${label}`);
		await new Promise((resolve) => setTimeout(resolve, 200));
	}
}

async function openPart(page: Page) {
	await page.goto(`http://127.0.0.1:${port}/#/part/${PART}`);
	await page.locator(`article[data-item="INT-01"]`).waitFor();
}

const card = (page: Page, id: string) => page.locator(`article[data-item="${id}"]`);
const blockAnswers = (page: Page) => page.route("**/api/answers/**", (route) => route.abort("connectionrefused"));

before(async () => {
	if (skip) return;
	root = await mkdtemp(path.join(os.tmpdir(), "lockdown-ui-"));
	await mkdir(path.join(root, "docs", "lockdown", "parts"), { recursive: true });
	const part = { id: PART, order: 1, title: "Purpose", status: "ready", summary: "Test part.", items: ["INT-01", "INT-02", "INT-03", "INT-04"].map(item) };
	await writeFile(path.join(root, "docs", "lockdown", "parts", `${PART}.json`), JSON.stringify(part));
	({ child: server, port } = await startServer(root));
	browser = await chromium!.launch();
});

after(async () => {
	await browser?.close();
	server?.kill();
	if (root) await rm(root, { recursive: true, force: true });
});

test("a failed decision is kept across a reload and saved once the server answers", { skip }, async () => {
	const context = await browser.newContext();
	const page = await context.newPage();
	await blockAnswers(page);
	await openPart(page);
	await card(page, "INT-01").locator("[data-act='accept'] >> visible=true").first().click();
	await until(async () => (await card(page, "INT-01").textContent())!.includes("Not saved"), "Not saved");
	assert.match((await page.locator("#unsaved").textContent())!, /1 change not saved/);
	assert.deepEqual(await readAnswers(), {});

	// Leaving the page warns while the change is unsaved.
	const prevented = await page.evaluate(() => {
		const event = new Event("beforeunload", { cancelable: true });
		window.dispatchEvent(event);
		return event.defaultPrevented;
	});
	assert.equal(prevented, true);

	// A reload with the server still unreachable for answers keeps the change.
	await page.reload();
	await card(page, "INT-01").waitFor();
	await until(async () => (await card(page, "INT-01").textContent())!.includes("Not saved"), "Not saved after reload");
	assert.deepEqual(await readAnswers(), {});

	// The server answers again: the next poll saves it without a click.
	await page.unroute("**/api/answers/**");
	await until(async () => (await readAnswers())["INT-01"]?.decision === "accept", "the retried save");
	await until(async () => page.locator("#unsaved").isHidden(), "the unsaved line to clear");
	await context.close();
});

test("a note typed before deciding survives a reload and is saved with the decision", { skip }, async () => {
	const context = await browser.newContext();
	const page = await context.newPage();
	await openPart(page);
	await card(page, "INT-02").locator("[data-act='note'] >> visible=true").first().click();
	await card(page, "INT-02").locator("textarea[data-role='note']").fill("Ask Steven first");
	assert.match((await page.locator("#unsaved").textContent())!, /1 note kept in this browser/);
	await page.reload();
	await card(page, "INT-02").waitFor();
	assert.equal(await card(page, "INT-02").locator("textarea[data-role='note']").inputValue(), "Ask Steven first");
	await card(page, "INT-02").locator("[data-act='defer'] >> visible=true").first().click();
	await until(async () => (await readAnswers())["INT-02"]?.note === "Ask Steven first", "the note saved with the decision");
	assert.equal((await readAnswers())["INT-02"].decision, "defer");
	await until(async () => page.locator("#unsaved").isHidden(), "the unsaved line to clear");
	await context.close();
});

test("a stale unsaved change never overwrites a newer saved answer", { skip }, async () => {
	const context = await browser.newContext();
	const page = await context.newPage();
	// Saved elsewhere (another tab or device) after this browser's change was made.
	const saved = await fetch(`http://127.0.0.1:${port}/api/answers/${PART}/INT-03`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ decision: "accept", note: "" }),
	}).then((response) => response.json());
	await page.goto(`http://127.0.0.1:${port}/`);
	await page.evaluate((part) => {
		const entry = { partId: part, itemId: "INT-03", intent: { decision: "reject", note: "", token: "old", base: null }, at: new Date().toISOString() };
		localStorage.setItem("lockdown.journal.v1", JSON.stringify({ entries: { [`${part}/INT-03`]: entry } }));
	}, PART);
	await openPart(page);
	await until(async () => (await card(page, "INT-03").textContent())!.includes("Keep the saved answer"), "the conflict line");
	// Several polls later the server still holds the newer answer.
	await new Promise((resolve) => setTimeout(resolve, 6000));
	const answers = await readAnswers();
	assert.equal(answers["INT-03"].decision, "accept");
	assert.equal(answers["INT-03"].updatedAt, saved.answer.updatedAt);
	await card(page, "INT-03").locator("[data-act='keep-saved']").click();
	assert.equal(await page.evaluate(() => localStorage.getItem("lockdown.journal.v1")), null);
	await context.close();
});

test("a failed edit is kept as typed and saved when the server answers", { skip }, async () => {
	const context = await browser.newContext();
	const page = await context.newPage();
	await blockAnswers(page);
	await openPart(page);
	await card(page, "INT-04").locator("[data-act='edit'] >> visible=true").first().click();
	await card(page, "INT-04").locator("textarea >> visible=true").first().fill("My own wording.");
	await until(async () => (await card(page, "INT-04").textContent())!.includes("Not saved"), "Not saved");
	// Leaving the part and coming back keeps the edit.
	await page.goto(`http://127.0.0.1:${port}/#/`);
	await page.unroute("**/api/answers/**");
	await openPart(page);
	await until(async () => (await readAnswers())["INT-04"]?.text === "My own wording.", "the edit saved");
	assert.equal((await readAnswers())["INT-04"].decision, "edit");
	await context.close();
});
