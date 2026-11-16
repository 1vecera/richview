// Drives cli.mjs against server.mjs on an ephemeral port and a temporary root with three parts and
// a map built by the real build-map.mjs.
// Run: node --test tools/lockdown-app/test/cli.test.mjs
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { after, before, test } from "node:test";

const APP = path.join(import.meta.dirname, "..");
const SERVER = path.join(APP, "server.mjs");
const CLI = path.join(APP, "cli.mjs");
const BUILD_MAP = path.join(APP, "..", "..", "docs", "lockdown", "map", "build-map.mjs");

let root;
let server;
let port;

const lockdown = (...parts) => path.join(root, "docs", "lockdown", ...parts);
const readAnswers = async (id) => JSON.parse(await readFile(lockdown("answers", `${id}.json`), "utf8")).answers;

function item(id, importance, extra = {}) {
	return {
		id,
		kind: "decision",
		importance,
		title: `Title ${id}`,
		question: `Question ${id}?`,
		why: `Why ${id}.`,
		proposed: `Proposal for ${id}.`,
		confidence: "stated",
		sources: [{ label: "Code", ref: "README.md:1" }],
		...extra,
	};
}

function writePart(id, order, items) {
	const part = { id, order, title: `Part ${id}`, status: "ready", summary: "Summary.", items };
	return writeFile(lockdown("parts", `${id}.json`), JSON.stringify(part, null, 2));
}

const OPTIONS = [
	{ label: "First", detail: "The first way." },
	{ label: "Second", detail: "The second way, with **detail**." },
];

const intent = () => [
	item("INT-01", "detail"),
	item("INT-02", "key", { refId: "D-TAGLINE", conflicts: "Answer OPN-50 first; it decides this." }),
	item("INT-03", "important", { options: OPTIONS, recommendation: "Second, because it is shorter.", conflicts: "Contradicts D-TAGLINE." }),
];

function startServer(dir) {
	return new Promise((resolve, reject) => {
		const child = spawn(process.execPath, [SERVER, "--port", "0", "--root", dir], { stdio: ["ignore", "pipe", "pipe"] });
		let output = "";
		const timer = setTimeout(() => {
			child.kill();
			reject(new Error(`The server did not start: ${output}`));
		}, 10_000);
		child.stdout.on("data", (chunk) => {
			output += chunk;
			const match = /http:\/\/127\.0\.0\.1:(\d+)\//.exec(output);
			if (match) {
				clearTimeout(timer);
				resolve({ child, port: Number(match[1]) });
			}
		});
		child.stderr.on("data", (chunk) => {
			output += chunk;
		});
	});
}

function cli(...args) {
	const result = spawnSync(process.execPath, [CLI, ...args, "--url", `http://127.0.0.1:${port}`, "--root", root], {
		encoding: "utf8",
		env: { ...process.env, LOCKDOWN_URL: "", LOCKDOWN_ROOT: "" },
	});
	return { code: result.status, out: result.stdout, err: result.stderr };
}

const json = (...args) => {
	const result = cli(...args, "--json");
	assert.equal(result.code, 0, result.err);
	return JSON.parse(result.out);
};

before(async () => {
	root = await mkdtemp(path.join(os.tmpdir(), "lockdown-cli-test-"));
	await mkdir(lockdown("parts"), { recursive: true });
	await mkdir(lockdown("answers"), { recursive: true });
	await mkdir(lockdown("map"), { recursive: true });
	await copyFile(BUILD_MAP, lockdown("map", "build-map.mjs"));
	await writePart("01-intent", 1, intent());
	await writePart("02-delivery", 2, [item("DEL-01", "key", { proposed: "Builds on INT-02." }), item("DEL-02", "detail")]);
	await writePart("08-open-work", 8, [
		item("OPN-01", "key", { group: "Production", why: "Touches INT-02." }),
		item("OPN-50", "key", { group: "Cross-part contradictions" }),
		item("OPN-51", "important", { group: "Cross-part contradictions" }),
	]);
	({ child: server, port } = await startServer(root));
});

after(async () => {
	server?.kill();
	await rm(root, { recursive: true, force: true });
});

test("--help prints the usage", () => {
	const result = cli("--help");
	assert.equal(result.code, 0);
	assert.match(result.out, /^lockdown CLI/);
	assert.match(result.out, /answer <ID> \(--accept/);
});

test("show prints the full card with its recommended option and deep link", () => {
	const result = cli("show", "int-03");
	assert.equal(result.code, 0, result.err);
	for (const text of [
		"INT-03 · Title INT-03",
		"01-intent · important · decision · confidence stated · open",
		"Why:\n  Why INT-03.",
		"  2. Second [recommended]\n     The second way, with **detail**.",
		"Recommendation:\n  Second, because it is shorter.",
		"Sources:\n  - Code: README.md:1",
		"Answer: none",
		`Link: http://localhost:${port}/#/part/01-intent/INT-03`,
	]) {
		assert.ok(result.out.includes(text), `missing ${JSON.stringify(text)} in:\n${result.out}`);
	}
	const [card] = json("show", "INT-03");
	assert.deepEqual(
		card.options.map((option) => [option.label, option.recommended]),
		[
			["First", false],
			["Second", true],
		],
	);
});

test("show --brief keeps the essentials and cuts the conflict to about 300 characters", async () => {
	const long = `Long conflict ${"x".repeat(600)}`;
	await writePart("02-delivery", 2, [item("DEL-01", "key", { proposed: "Builds on INT-02." }), item("DEL-02", "detail", { conflicts: long })]);
	const result = cli("show", "DEL-02", "INT-03", "--brief");
	assert.equal(result.code, 0, result.err);
	assert.ok(!result.out.includes("Why:"));
	assert.ok(!result.out.includes("Sources:"));
	assert.ok(result.out.includes("Question:\n  Question DEL-02?"));
	assert.ok(result.out.includes("  2. Second [recommended]\nRecommendation:"));
	const conflict = /Conflicts:\n {2}(.*)\n/.exec(result.out)[1];
	assert.ok(conflict.length <= 300 && conflict.endsWith("…"), conflict);
	await writePart("02-delivery", 2, [item("DEL-01", "key", { proposed: "Builds on INT-02." }), item("DEL-02", "detail")]);
});

test("next orders by importance, then cross-part cards, Part 01, Part 02, the rest, then file order", () => {
	const ids = (...args) => json("next", ...args).map((card) => card.id);
	assert.deepEqual(ids("-n", "20"), ["OPN-50", "INT-02", "DEL-01", "OPN-01", "OPN-51", "INT-03", "INT-01", "DEL-02"]);
	assert.deepEqual(ids(), ["OPN-50", "INT-02", "DEL-01", "OPN-01", "OPN-51"]);
	assert.deepEqual(ids("--part", "01-intent"), ["INT-02", "INT-03", "INT-01"]);
	assert.deepEqual(ids("--importance", "important,detail", "--skip", "OPN-51,int-01"), ["INT-03", "DEL-02"]);
	assert.match(cli("next", "-n", "1").out, /^OPN-50 +key open +08-open-work +Title OPN-50\n\(8 unanswered match\)/);
});

test("list and search filter in review order", () => {
	assert.deepEqual(
		json("list", "--importance", "key", "--part", "08").map((card) => card.id),
		["OPN-50", "OPN-01"],
	);
	assert.deepEqual(
		json("search", "builds", "int-02").map((card) => card.id),
		["DEL-01"],
	);
	assert.deepEqual(
		json("list", "--search", "proposal for opn").map((card) => card.id),
		["OPN-50", "OPN-01", "OPN-51"],
	);
	assert.equal(cli("list", "--answered", "--unanswered").code, 2);
});

test("deps groups the map's links, rebuilding the map when a part is newer", async () => {
	const deps = json("deps", "INT-02");
	const ids = (list) => list.map((entry) => `${entry.direction}:${entry.id}`);
	assert.deepEqual(ids(deps.decidedBy), ["in:OPN-50"]);
	assert.deepEqual(ids(deps.conflicts), ["out:OPN-50", "in:INT-03"]);
	assert.deepEqual(ids(deps.dependedOnBy), ["in:DEL-01"]);
	assert.deepEqual(ids(deps.mentions), ["in:OPN-01"]);
	assert.deepEqual(deps.dependsOn, []);
	const text = cli("deps", "INT-02").out;
	assert.match(text, /Decided by \(1\)\n {2}OPN-50 +key open +Title OPN-50/);
	assert.match(text, /Conflicts \(2\)\n {2}-> OPN-50 .*\n {2}<- INT-03 +imp open/);

	const built = (await stat(lockdown("map", "map.json"))).mtimeMs;
	await new Promise((resolve) => setTimeout(resolve, 30));
	await writePart("02-delivery", 2, [
		item("DEL-01", "key", { proposed: "Builds on INT-02." }),
		item("DEL-02", "detail"),
		item("DEL-03", "important", { question: "Does INT-02 hold?" }),
	]);
	assert.deepEqual(ids(json("deps", "INT-02").dependedOnBy), ["in:DEL-01", "in:DEL-03"]);
	assert.ok((await stat(lockdown("map", "map.json"))).mtimeMs > built);
});

test("ripple lists unanswered decides, conflict and depends referrers key first", () => {
	const ripple = json("ripple", "INT-02");
	assert.deepEqual(
		ripple.cards.map((entry) => [entry.id, entry.type]),
		[
			["OPN-50", "decides"],
			["DEL-01", "depends"],
			["INT-03", "conflict"],
			["DEL-03", "depends"],
		],
	);
	assert.match(cli("ripple", "INT-02").out, /^Ripple of INT-02: 4 unanswered cards to recheck\n {2}decides +OPN-50 +key open/);
});

test("answer --accept saves through the API and prints the answer and its ripple", async () => {
	const result = cli("answer", "DEL-01", "--accept");
	assert.equal(result.code, 0, result.err);
	assert.match(result.out, /^Saved DEL-01 \(02-delivery\) as accept\.\nAnswer: accept, updated /);
	assert.match(result.out, /Ripple of DEL-01: no unanswered card references it/);
	assert.equal((await readAnswers("02-delivery"))["DEL-01"].decision, "accept");
	const ripple = json("ripple", "INT-02");
	assert.ok(!ripple.cards.some((entry) => entry.id === "DEL-01"));
	assert.ok(json("ripple", "INT-02", "--all").cards.some((entry) => entry.id === "DEL-01" && entry.state === "accept"));
	assert.ok(!json("next", "-n", "20").some((card) => card.id === "DEL-01"));
});

test("answer refuses to replace an answer without --force and prints it first with --force", async () => {
	const before = await readFile(lockdown("answers", "log.jsonl"), "utf8");
	const refused = cli("answer", "DEL-01", "--reject");
	assert.equal(refused.code, 1);
	assert.match(refused.err, /^Refused: DEL-01 already has an answer\. Pass --force to replace it\.\nAnswer: accept/);
	assert.equal((await readAnswers("02-delivery"))["DEL-01"].decision, "accept");
	assert.equal(await readFile(lockdown("answers", "log.jsonl"), "utf8"), before);

	const forced = cli("answer", "DEL-01", "--defer", "--force");
	assert.equal(forced.code, 0, forced.err);
	assert.match(forced.out, /^Replacing the existing answer of DEL-01:\nAnswer: accept, .*\n\nSaved DEL-01 \(02-delivery\) as defer\./);
	assert.equal((await readAnswers("02-delivery"))["DEL-01"].decision, "defer");
});

test("answer --edit and --edit-file save the text", async () => {
	assert.equal(cli("answer", "INT-01", "--edit", "My own wording.").code, 0);
	assert.equal((await readAnswers("01-intent"))["INT-01"].text, "My own wording.");
	const file = path.join(root, "edit.md");
	await writeFile(file, "From a file.\n\nSecond paragraph.\n");
	const result = cli("answer", "INT-01", "--edit-file", file, "--force");
	assert.equal(result.code, 0, result.err);
	const saved = (await readAnswers("01-intent"))["INT-01"];
	assert.equal(saved.decision, "edit");
	assert.equal(saved.text, "From a file.\n\nSecond paragraph.");
});

test("answer --choose saves what the app's Choose button saves, by label or index", async () => {
	const result = cli("answer", "INT-03", "--choose", "second");
	assert.equal(result.code, 0, result.err);
	assert.match(result.out, /^Saved INT-03 \(01-intent\) as chosen\./);
	const saved = (await readAnswers("01-intent"))["INT-03"];
	assert.equal(saved.decision, "edit");
	assert.equal(saved.text, "Chosen option: Second.\n\nThe second way, with **detail**.");
	assert.match(cli("show", "INT-03").out, /2\. Second \[recommended, chosen\]/);

	assert.equal(cli("answer", "INT-03", "--choose", "1", "--force").code, 0);
	assert.equal((await readAnswers("01-intent"))["INT-03"].text, "Chosen option: First.\n\nThe first way.");
	const bad = cli("answer", "INT-03", "--choose", "Third", "--force");
	assert.equal(bad.code, 1);
	assert.match(bad.err, /No options of INT-03 match "Third":\n {2}1\. First\n {2}2\. Second/);
	assert.equal(cli("answer", "INT-01", "--choose", "1", "--force").code, 1);
});

test("answer --dictated records Daniel's words in the note, with --note added", async () => {
	const result = cli("answer", "OPN-50", "--accept", "--dictated", "Keep part 07's rule.", "--note", "Applies to WebKit too.");
	assert.equal(result.code, 0, result.err);
	const now = new Date();
	const date = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
	const saved = (await readAnswers("08-open-work"))["OPN-50"];
	assert.equal(saved.decision, "accept");
	assert.equal(
		saved.note,
		`Dictated by Daniel in the review chat on ${date}: "Keep part 07's rule." Recorded by the lockdown agent at his request.\n\nApplies to WebKit too.`,
	);
	assert.ok(result.out.includes(`Note:\n  Dictated by Daniel in the review chat on ${date}`));

	assert.equal(cli("answer", "OPN-51", "--defer", "--dictated", "later").code, 0);
	assert.match((await readAnswers("08-open-work"))["OPN-51"].note, /: "later"\. Recorded by the lockdown agent at his request\.$/);
	assert.equal(cli("clear", "OPN-51", "--force").code, 0);

	assert.equal(cli("answer", "OPN-50", "--reject", "--force").code, 0);
	assert.equal((await readAnswers("08-open-work"))["OPN-50"].note, saved.note, "--force without a new note keeps the note");
});

test("answer and clear validate their arguments", async () => {
	assert.equal(cli("answer", "OPN-51").code, 2);
	assert.equal(cli("answer", "OPN-51", "--accept", "--reject").code, 2);
	assert.equal(cli("answer", "NOPE-1", "--accept").code, 1);
	assert.equal(cli("clear", "OPN-50").code, 2);
	const cleared = cli("clear", "OPN-50", "--force");
	assert.equal(cleared.code, 0, cleared.err);
	assert.match(cleared.out, /^Cleared OPN-50\. It held:\nAnswer: reject/);
	assert.equal((await readAnswers("08-open-work"))["OPN-50"], undefined);
	assert.match(cli("clear", "OPN-50", "--force").out, /has no answer/);
});

test("status counts answers overall, per part, for key and cross-part cards", () => {
	const status = json("status");
	assert.equal(status.totals.itemCount, 9);
	assert.equal(status.totals.answeredCount, 3);
	assert.deepEqual(status.totals.decisions, { accept: 0, edit: 2, reject: 0, defer: 1 });
	assert.equal(status.totals.chosen, 1);
	assert.deepEqual(status.key, { itemCount: 4, answeredCount: 1 });
	assert.deepEqual(status.crossPart, { itemCount: 2, answeredCount: 0 });
	assert.match(cli("status").out, /^Answered 3\/9 · key 1\/4 · cross-part \(OPN-50\.\.68\) 0\/2\n/);
	assert.equal(cli("open", "opn-51").out.trim(), `http://localhost:${port}/#/part/08-open-work/OPN-51`);
});

test("a server that cannot be reached is an error, not a crash", () => {
	const result = spawnSync(process.execPath, [CLI, "status", "--url", "http://127.0.0.1:9"], { encoding: "utf8" });
	assert.equal(result.status, 1);
	assert.match(result.stderr, /Cannot reach the lockdown app at http:\/\/127\.0\.0\.1:9/);
});
