// Starts server.mjs on an ephemeral port against a temporary root and checks its contract.
// Run: node --test tools/lockdown-app/test/server.test.mjs
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { request } from "node:http";
import os from "node:os";
import path from "node:path";
import { after, before, test } from "node:test";

const SERVER = path.join(import.meta.dirname, "..", "server.mjs");
const PNG = Buffer.from(
	"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=",
	"base64",
);

let root;
let server;
let port;

const partsDir = () => path.join(root, "docs", "lockdown", "parts");
const answersFile = (id) => path.join(root, "docs", "lockdown", "answers", `${id}.json`);
const readAnswers = async (id) => JSON.parse(await readFile(answersFile(id), "utf8"));

function item(id, extra = {}) {
	return {
		id,
		kind: "feature",
		title: `Title ${id}`,
		question: "Keep it?",
		proposed: `Proposal for ${id}.`,
		confidence: "observed",
		sources: [{ label: "Code", ref: "README.md:1" }],
		...extra,
	};
}

function writePart(id, order, items, extra = {}) {
	const part = { id, order, title: `Part ${id}`, status: "draft", summary: "What this part settles.", items, ...extra };
	return writeFile(path.join(partsDir(), `${id}.json`), JSON.stringify(part, null, 2));
}

// Runs git without the developer's hooks or signing so a test repository commits anywhere.
function git(cwd, ...args) {
	const config = ["user.name=Lockdown test", "user.email=test@example.invalid", "commit.gpgsign=false", "core.hooksPath=/dev/null"];
	return execFileSync("git", [...config.flatMap((entry) => ["-c", entry]), ...args], { cwd, stdio: ["ignore", "pipe", "pipe"] });
}

async function readLog() {
	try {
		const raw = await readFile(path.join(root, "docs", "lockdown", "answers", "log.jsonl"), "utf8");
		return raw.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
	} catch (error) {
		if (error.code === "ENOENT") return [];
		throw error;
	}
}

function startServer(dir) {
	return new Promise((resolve, reject) => {
		const child = spawn(process.execPath, [SERVER, "--port", "0", "--root", dir], { stdio: ["ignore", "pipe", "pipe"] });
		let output = "";
		const timer = setTimeout(() => {
			child.kill();
			reject(new Error(`The server did not start: ${output}`));
		}, 10_000);
		const onData = (chunk) => {
			output += chunk;
			const match = /http:\/\/127\.0\.0\.1:(\d+)\//.exec(output);
			if (match) {
				clearTimeout(timer);
				resolve({ child, port: Number(match[1]) });
			}
		};
		child.stdout.on("data", onData);
		child.stderr.on("data", (chunk) => {
			output += chunk;
		});
		child.on("exit", (code) => {
			clearTimeout(timer);
			reject(new Error(`The server exited with ${code}: ${output}`));
		});
	});
}

// `at` addresses a second server started by one test; the shared server is the default.
function http(method, pathname, { body, headers = {}, at = port } = {}) {
	return new Promise((resolve, reject) => {
		const payload = body === undefined ? undefined : typeof body === "string" ? body : JSON.stringify(body);
		const req = request(
			{
				host: "127.0.0.1",
				port: at,
				method,
				path: pathname,
				headers: {
					Host: `127.0.0.1:${at}`,
					...(payload === undefined
						? {}
						: { "Content-Type": "application/json", "Content-Length": Buffer.byteLength(payload) }),
					...headers,
				},
			},
			(res) => {
				const chunks = [];
				res.on("data", (chunk) => chunks.push(chunk));
				res.on("end", () => {
					const buffer = Buffer.concat(chunks);
					const text = buffer.toString("utf8");
					let json;
					try {
						json = JSON.parse(text);
					} catch {}
					resolve({ status: res.statusCode, headers: res.headers, buffer, text, json });
				});
			},
		);
		req.on("error", reject);
		if (payload !== undefined) req.write(payload);
		req.end();
	});
}

before(async () => {
	root = await mkdtemp(path.join(os.tmpdir(), "lockdown-app-test-"));
	await mkdir(partsDir(), { recursive: true });
	await mkdir(path.join(root, "docs", "lockdown", "screenshots"), { recursive: true });
	await mkdir(path.join(root, "docs", "guide"), { recursive: true });
	await writePart("01-alpha", 1, [item("A-01"), item("A-02"), item("A-03")], { status: "ready" });
	await writePart("02-beta", 2, [item("B-01"), item("B-02", { kind: "style" })]);
	await writeFile(path.join(partsDir(), "03-broken.json"), '{"id": "03-broken", "items": [{"id": "C-01"');
	await writeFile(path.join(partsDir(), ".03-broken.json.tmp"), "{}");
	await writeFile(path.join(root, "docs", "lockdown", "screenshots", "shot.png"), PNG);
	await writeFile(path.join(root, "docs", "lockdown", "screenshots", "notes.txt"), "not an image");
	await writeFile(path.join(root, "docs", "guide", "diagram.png"), PNG);
	await writeFile(path.join(root, "secret.png"), PNG);
	await symlink(path.join(root, "secret.png"), path.join(root, "docs", "lockdown", "screenshots", "escape.png"));
	({ child: server, port } = await startServer(root));
});

after(async () => {
	server?.kill();
	await rm(root, { recursive: true, force: true });
});

test("lists parts in order with counts and marks a malformed part unavailable", async () => {
	const res = await http("GET", "/api/parts");
	assert.equal(res.status, 200);
	assert.equal(res.headers["cache-control"], "no-store");
	assert.deepEqual(
		res.json.parts.map((part) => part.id),
		["01-alpha", "02-beta", "03-broken"],
	);
	const [alpha, , broken] = res.json.parts;
	assert.equal(alpha.title, "Part 01-alpha");
	assert.equal(alpha.status, "ready");
	assert.equal(alpha.order, 1);
	assert.equal(alpha.itemCount, 3);
	assert.equal(alpha.answeredCount, 0);
	assert.deepEqual(alpha.decisions, { accept: 0, edit: 0, reject: 0, defer: 0 });
	assert.equal(alpha.error, null);
	assert.equal(broken.status, "unavailable");
	assert.match(broken.error, /^Invalid JSON/);
	assert.equal(broken.itemCount, 0);
	const none = { itemCount: 0, answeredCount: 0 };
	assert.deepEqual(res.json.totals, {
		parts: 3,
		ready: 1,
		itemCount: 5,
		answeredCount: 0,
		decisions: { accept: 0, edit: 0, reject: 0, defer: 0 },
		importance: { key: none, important: none, detail: none, unrated: { itemCount: 5, answeredCount: 0 } },
	});
});

test("serves the UI with a restrictive content security policy", async () => {
	const res = await http("GET", "/");
	assert.equal(res.status, 200);
	assert.match(res.headers["content-type"], /^text\/html/);
	assert.match(res.headers["content-security-policy"], /script-src 'self'/);
	assert.equal((await http("GET", "/app.js")).status, 200);
	assert.equal((await http("GET", "/missing.js")).status, 404);
	assert.equal((await http("GET", "/..%2fserver.mjs")).status, 404);
});

test("PUT stores an answer, merges it into the part and appends to the log", async () => {
	const res = await http("PUT", "/api/answers/01-alpha/A-01", { body: { decision: "accept", note: "  Looks right.  " } });
	assert.equal(res.status, 200);
	assert.equal(res.json.answer.decision, "accept");
	assert.equal(res.json.answer.note, "Looks right.");
	assert.equal(res.json.answer.stale, false);

	const file = await readAnswers("01-alpha");
	assert.equal(file.partId, "01-alpha");
	assert.equal(file.answers["A-01"].decision, "accept");
	assert.equal(file.answers["A-01"].text, undefined);
	assert.ok(Date.parse(file.answers["A-01"].updatedAt));

	const merged = await http("GET", "/api/parts/01-alpha");
	assert.equal(merged.status, 200);
	assert.equal(merged.json.items[0].answer.decision, "accept");
	assert.equal(merged.json.items[1].answer, null);
	assert.equal(merged.json.answeredCount, 1);
	assert.equal(merged.json.items[0].sources[0].ref, "README.md:1");

	const entry = (await readLog()).at(-1);
	assert.deepEqual(
		{ ...entry, at: undefined },
		{ at: undefined, partId: "01-alpha", itemId: "A-01", decision: "accept", text: null, note: "Looks right.", action: "set" },
	);
});

test("an edit stores its replacement text and needs text", async () => {
	const empty = await http("PUT", "/api/answers/01-alpha/A-02", { body: { decision: "edit", text: "  " } });
	assert.equal(empty.status, 400);
	const res = await http("PUT", "/api/answers/01-alpha/A-02", { body: { decision: "edit", text: "My version.\n" } });
	assert.equal(res.status, 200);
	assert.equal((await readAnswers("01-alpha")).answers["A-02"].text, "My version.");
	assert.equal((await readLog()).at(-1).text, "My version.");
	const accepted = await http("PUT", "/api/answers/01-alpha/A-02", { body: { decision: "accept", text: "ignored" } });
	assert.equal(accepted.json.answer.text, undefined);
	await http("PUT", "/api/answers/01-alpha/A-02", { body: { decision: "edit", text: "My version." } });
});

test("rejects an invalid decision, unknown items and parts, and non-JSON bodies without writing", async () => {
	const before = await readLog();
	const invalid = await http("PUT", "/api/answers/01-alpha/A-03", { body: { decision: "maybe" } });
	assert.equal(invalid.status, 400);
	assert.match(invalid.json.error, /decision must be/);
	assert.equal((await http("PUT", "/api/answers/01-alpha/A-99", { body: { decision: "accept" } })).status, 404);
	assert.equal((await http("PUT", "/api/answers/99-missing/A-01", { body: { decision: "accept" } })).status, 404);
	assert.equal((await http("PUT", "/api/answers/01-alpha/A-03", { body: { decision: "accept", note: 5 } })).status, 400);
	const textBody = await http("PUT", "/api/answers/01-alpha/A-03", {
		body: '{"decision":"accept"}',
		headers: { "Content-Type": "text/plain" },
	});
	assert.equal(textBody.status, 415);
	assert.equal((await http("PUT", "/api/answers/01-alpha/A-03", { body: "{not json" })).status, 400);
	assert.deepEqual(await readLog(), before);
	assert.equal((await readAnswers("01-alpha")).answers["A-03"], undefined);
});

test("bulk saves every entry or nothing", async () => {
	const invalid = await http("POST", "/api/answers/02-beta/bulk", {
		body: [
			{ itemId: "B-01", decision: "accept" },
			{ itemId: "B-02", decision: "nope" },
		],
	});
	assert.equal(invalid.status, 400);
	assert.equal(invalid.json.details.length, 1);
	const unknown = await http("POST", "/api/answers/02-beta/bulk", {
		body: [
			{ itemId: "B-01", decision: "accept" },
			{ itemId: "B-77", decision: "accept" },
		],
	});
	assert.equal(unknown.status, 404);
	await assert.rejects(readFile(answersFile("02-beta")), { code: "ENOENT" });
	assert.equal((await http("POST", "/api/answers/02-beta/bulk", { body: [] })).status, 400);

	const logBefore = await readLog();
	const res = await http("POST", "/api/answers/02-beta/bulk", {
		body: [
			{ itemId: "B-01", decision: "accept" },
			{ itemId: "B-02", decision: "defer", note: "Later." },
		],
	});
	assert.equal(res.status, 200);
	assert.deepEqual(Object.keys(res.json.answers), ["B-01", "B-02"]);
	const file = await readAnswers("02-beta");
	assert.equal(file.answers["B-01"].decision, "accept");
	assert.equal(file.answers["B-02"].note, "Later.");
	const log = await readLog();
	assert.equal(log.length, logBefore.length + 2);
	assert.ok(log.slice(-2).every((entry) => entry.bulk === true && entry.partId === "02-beta"));

	const listing = await http("GET", "/api/parts");
	const beta = listing.json.parts.find((part) => part.id === "02-beta");
	assert.equal(beta.answeredCount, 2);
	assert.deepEqual(beta.decisions, { accept: 1, edit: 0, reject: 0, defer: 1 });
});

test("DELETE clears an answer and logs the clear", async () => {
	const res = await http("DELETE", "/api/answers/02-beta/B-02");
	assert.equal(res.status, 200);
	assert.equal(res.json.answer, null);
	assert.equal((await readAnswers("02-beta")).answers["B-02"], undefined);
	const entry = (await readLog()).at(-1);
	assert.equal(entry.itemId, "B-02");
	assert.equal(entry.decision, null);
	assert.equal(entry.action, "clear");
	const logLength = (await readLog()).length;
	assert.equal((await http("DELETE", "/api/answers/02-beta/B-02")).status, 200);
	assert.equal((await readLog()).length, logLength, "clearing an empty answer writes nothing");
	assert.equal((await http("DELETE", "/api/answers/02-beta/B-99")).status, 404);
});

test("concurrent writes to one part are serialized and none is lost", async () => {
	const ids = Array.from({ length: 40 }, (_, index) => `M-${String(index + 1).padStart(2, "0")}`);
	await writePart("04-many", 4, ids.map((id) => item(id)));
	const logBefore = (await readLog()).length;
	const results = await Promise.all(
		ids.map((id, index) =>
			http("PUT", `/api/answers/04-many/${id}`, { body: { decision: index % 2 ? "reject" : "accept" } }),
		),
	);
	assert.ok(results.every((res) => res.status === 200));
	const file = await readAnswers("04-many");
	assert.deepEqual(Object.keys(file.answers), ids, "answers follow the part's item order");
	assert.equal((await readLog()).length, logBefore + ids.length);
});

test("rejects requests with a foreign Host header", async () => {
	for (const host of ["evil.example", "evil.example:80", `127.0.0.1.evil.example:${port}`, "localhost.evil.example"]) {
		const res = await http("GET", "/api/parts", { headers: { Host: host } });
		assert.equal(res.status, 403, `Host ${JSON.stringify(host)}`);
	}
	const write = await http("PUT", "/api/answers/01-alpha/A-03", {
		body: { decision: "accept" },
		headers: { Host: "attacker.example:4790" },
	});
	assert.equal(write.status, 403);
	assert.equal((await readAnswers("01-alpha")).answers["A-03"], undefined);
	const forwarded = await http("GET", "/api/parts", { headers: { Host: "localhost:14790" } });
	assert.equal(forwarded.status, 200, "an SSH port forward keeps working");
});

test("rejects cross-origin writes and accepts same-origin ones", async () => {
	for (const origin of ["http://evil.example", `http://localhost:${port + 1}`, "null", `https://127.0.0.1:${port}`]) {
		const res = await http("PUT", "/api/answers/01-alpha/A-03", {
			body: { decision: "reject" },
			headers: { Origin: origin },
		});
		assert.equal(res.status, 403, `Origin ${origin}`);
	}
	const bulk = await http("POST", "/api/answers/01-alpha/bulk", {
		body: [{ itemId: "A-03", decision: "reject" }],
		headers: { Origin: "http://evil.example" },
	});
	assert.equal(bulk.status, 403);
	const remove = await http("DELETE", "/api/answers/01-alpha/A-01", { headers: { Origin: "http://evil.example" } });
	assert.equal(remove.status, 403);
	const file = await readAnswers("01-alpha");
	assert.equal(file.answers["A-03"], undefined);
	assert.equal(file.answers["A-01"].decision, "accept");

	const same = await http("PUT", "/api/answers/01-alpha/A-03", {
		body: { decision: "reject" },
		headers: { Origin: `http://127.0.0.1:${port}` },
	});
	assert.equal(same.status, 200);
	const forwarded = await http("PUT", "/api/answers/01-alpha/A-03", {
		body: { decision: "reject" },
		headers: { Host: "localhost:14790", Origin: "http://localhost:14790" },
	});
	assert.equal(forwarded.status, 200);
});

test("serves only images from the screenshot and guide folders", async () => {
	const shot = await http("GET", "/files/docs/lockdown/screenshots/shot.png");
	assert.equal(shot.status, 200);
	assert.equal(shot.headers["content-type"], "image/png");
	assert.deepEqual(shot.buffer, PNG);
	assert.equal((await http("GET", "/files/docs/guide/diagram.png")).status, 200);

	const blocked = [
		"/files/docs/lockdown/screenshots/notes.txt",
		"/files/secret.png",
		"/files/docs/lockdown/screenshots/../../../secret.png",
		"/files/docs/lockdown/screenshots/..%2f..%2f..%2fsecret.png",
		"/files/docs/lockdown/screenshots/%2e%2e/%2e%2e/%2e%2e/secret.png",
		"/files/docs/lockdown/screenshots/%2E%2E%2F%2E%2E%2F%2E%2E%2Fsecret.png",
		"/files/docs/lockdown/screenshots/escape.png",
		"/files/docs/lockdown/answers/01-alpha.json",
		"/files/docs/lockdown/parts/01-alpha.json",
		"/files/%2Fetc%2Fhosts",
		"/files/docs/guide/..%5c..%5csecret.png",
		"/files/docs/guide/diagram.png%00.txt",
	];
	for (const pathname of blocked) {
		const res = await http("GET", pathname);
		assert.equal(res.status, 404, pathname);
		assert.ok(!res.buffer.equals(PNG), pathname);
	}
	assert.equal((await http("GET", "/files/docs/guide/%E0%A4%A")).status, 400);
});

test("a malformed part is reported, refuses writes and recovers without a restart", async () => {
	const broken = await http("GET", "/api/parts/03-broken");
	assert.equal(broken.status, 200);
	assert.equal(broken.json.status, "unavailable");
	assert.match(broken.json.error, /^Invalid JSON/);
	assert.deepEqual(broken.json.items, []);
	const write = await http("PUT", "/api/answers/03-broken/C-01", { body: { decision: "accept" } });
	assert.equal(write.status, 409);
	assert.equal((await http("GET", "/api/parts/does-not-exist")).status, 404);

	await writeFile(path.join(partsDir(), "05-array.json"), "[]");
	const listing = await http("GET", "/api/parts");
	assert.match(listing.json.parts.find((part) => part.id === "05-array").error, /JSON object/);

	await writePart("03-broken", 3, [item("C-01")]);
	const fixed = await http("GET", "/api/parts/03-broken");
	assert.equal(fixed.json.error, null);
	assert.equal(fixed.json.items[0].id, "C-01");
	assert.equal((await http("PUT", "/api/answers/03-broken/C-01", { body: { decision: "accept" } })).status, 200);
});

test("a corrupt answers file is reported and never overwritten", async () => {
	await writePart("06-guarded", 6, [item("G-01")]);
	await mkdir(path.dirname(answersFile("06-guarded")), { recursive: true });
	await writeFile(answersFile("06-guarded"), "{ hand edited");
	const part = await http("GET", "/api/parts/06-guarded");
	assert.match(part.json.answersError, /not valid/);
	const write = await http("PUT", "/api/answers/06-guarded/G-01", { body: { decision: "accept" } });
	assert.equal(write.status, 409);
	assert.equal(await readFile(answersFile("06-guarded"), "utf8"), "{ hand edited");
});

test("flags answers whose proposal changed and keeps answers whose item left the part", async () => {
	await writePart("01-alpha", 1, [item("A-01", { proposed: "A revised proposal." }), item("A-02")], { status: "ready" });
	const part = await http("GET", "/api/parts/01-alpha");
	assert.equal(part.json.items[0].answer.stale, true);
	assert.equal(part.json.items[1].answer.stale, false);
	assert.deepEqual(
		part.json.orphans.map((orphan) => orphan.itemId),
		["A-03"],
	);
	const listing = await http("GET", "/api/parts");
	const alpha = listing.json.parts.find((entry) => entry.id === "01-alpha");
	assert.equal(alpha.staleCount, 1);
	assert.equal(alpha.orphanCount, 1);

	const reconfirmed = await http("PUT", "/api/answers/01-alpha/A-01", { body: { decision: "accept" } });
	assert.equal(reconfirmed.json.answer.stale, false);
	assert.equal((await http("DELETE", "/api/answers/01-alpha/A-03")).status, 200);
	assert.deepEqual((await http("GET", "/api/parts/01-alpha")).json.orphans, []);
});

test("export.md digests every part with final decisions and text", async () => {
	await writeFile(path.join(partsDir(), "07-halfway.json"), '{"items": [');
	const res = await http("GET", "/api/export.md");
	assert.equal(res.status, 200);
	assert.match(res.headers["content-type"], /^text\/plain/);
	const text = res.text;
	assert.match(text, /^# RichView lockdown answers\n/);
	assert.match(text, /## 01-alpha: Part 01-alpha\n\nStatus: ready\. 2 of 2 answered/);
	assert.match(text, /### A-01: Title A-01\n\n- Decision: accept\n/);
	assert.match(text, /Accepted text:\n\n> A revised proposal\./);
	assert.match(text, /### A-02: Title A-02\n\n- Decision: edit\n[\s\S]*?Edited text:\n\n> My version\./);
	assert.match(text, /### B-02: Title B-02\n\n- Decision: open\n[\s\S]*?Open proposal:\n\n> Proposal for B-02\./);
	assert.match(text, /## 07-halfway: 07-halfway\n\nUnavailable: Invalid JSON/);
	const single = await http("GET", "/api/export.md?part=02-beta");
	assert.match(single.text, /## 02-beta/);
	assert.doesNotMatch(single.text, /## 01-alpha/);
	assert.equal((await http("GET", "/api/export.md?part=nope")).status, 404);
});

test("export.md names each item's component with the title of its component card", async () => {
	await writePart("08-components", 8, [item("K-01", { kind: "component", refId: "C-ENGINE", title: "Chart engine" })]);
	await writePart("09-features", 9, [
		item("F-01", { refId: "F-LINE", component: "C-ENGINE" }),
		item("F-02", { component: " C-GONE " }),
		item("F-03"),
	]);
	const part = await http("GET", "/api/parts/09-features");
	assert.deepEqual(
		part.json.items.map((entry) => entry.component),
		["C-ENGINE", "C-GONE", ""],
	);
	const res = await http("GET", "/api/export.md?part=09-features");
	assert.equal(res.status, 200);
	assert.doesNotMatch(res.text, /## 08-components/);
	assert.match(res.text, /### F-01: Title F-01\n\n- Decision: open\n- Entity: F-LINE\n- Component: C-ENGINE Chart engine\n- Kind: feature;/);
	assert.match(res.text, /### F-02: Title F-02\n\n- Decision: open\n- Component: C-GONE \(no component card has this ID\)\n- Kind:/);
	assert.match(res.text, /### F-03: Title F-03\n\n- Decision: open\n- Kind:/);
});

test("lists guide and lockdown documents with titles from their first heading", async () => {
	const lockdown = path.join(root, "docs", "lockdown");
	const guide = path.join(root, "docs", "guide");
	await mkdir(path.join(lockdown, "evidence"), { recursive: true });
	await mkdir(path.join(guide, "components"), { recursive: true });
	await mkdir(path.join(root, "outside-dir"), { recursive: true });
	await writeFile(path.join(root, "outside.md"), "# Outside secret\n");
	await writeFile(path.join(root, "outside-dir", "x.md"), "# Linked secret\n");
	await writeFile(path.join(lockdown, "README.md"), "# RichView lockdown\n\nHow the lockdown works.\n");
	await writeFile(
		path.join(lockdown, "GOALS.md"),
		"---\ntitle: ignored\n---\n\n```md\n# Not the title\n```\n\n## Goals of the **rewrite**\n",
	);
	await writeFile(path.join(lockdown, "notes.md"), "# Not a lockdown file\n");
	await writeFile(path.join(lockdown, "evidence", "product.md"), "# Evidence\n");
	await symlink(path.join(root, "outside.md"), path.join(lockdown, "RULES.md"));
	await writeFile(path.join(guide, "README.md"), "# RichView guide\n");
	await writeFile(path.join(guide, "02-architecture.md"), "Intro line.\n\n# Architecture of `richview-core`\n");
	await writeFile(path.join(guide, "10-later.md"), "# Later\n");
	await writeFile(path.join(guide, "untitled.md"), "No heading here.\n");
	await writeFile(path.join(guide, "components", "README.md"), "# Components\n");
	await writeFile(path.join(guide, "components", "charts.md"), "# Charts [engine](../02-architecture.md)\n");
	await writeFile(path.join(guide, ".draft.md"), "# Draft\n");
	await writeFile(path.join(guide, "notes.txt"), "# Text\n");
	await symlink(path.join(root, "outside.md"), path.join(guide, "escape.md"));
	await symlink(path.join(guide, "README.md"), path.join(guide, "alias.md"));
	await symlink(path.join(root, "outside-dir"), path.join(guide, "linked"));

	const res = await http("GET", "/api/docs");
	assert.equal(res.status, 200);
	assert.equal(res.headers["cache-control"], "no-store");
	assert.deepEqual(
		res.json.docs.map((doc) => doc.path),
		[
			"docs/guide/README.md",
			"docs/guide/02-architecture.md",
			"docs/guide/10-later.md",
			"docs/guide/alias.md",
			"docs/guide/components/README.md",
			"docs/guide/components/charts.md",
			"docs/guide/untitled.md",
			"docs/lockdown/README.md",
			"docs/lockdown/GOALS.md",
		],
	);
	const byPath = Object.fromEntries(res.json.docs.map((doc) => [doc.path, doc]));
	assert.equal(byPath["docs/guide/README.md"].title, "RichView guide");
	assert.equal(byPath["docs/guide/02-architecture.md"].title, "Architecture of richview-core");
	assert.equal(byPath["docs/guide/components/charts.md"].title, "Charts engine");
	assert.equal(byPath["docs/guide/untitled.md"].title, "untitled.md");
	assert.equal(byPath["docs/lockdown/GOALS.md"].title, "Goals of the rewrite");
	assert.equal(byPath["docs/lockdown/README.md"].section, "lockdown");
	assert.equal(byPath["docs/guide/README.md"].section, "guide");
	assert.match(byPath["docs/guide/README.md"].revision, /^[0-9a-f]{16}$/);
	assert.ok(Date.parse(byPath["docs/guide/README.md"].updatedAt));
	assert.equal(byPath["docs/guide/README.md"].size, "# RichView guide\n".length);
});

test("serves an allowed document as raw Markdown and refuses everything else", async () => {
	const doc = await http("GET", "/api/docs/docs/guide/components/charts.md");
	assert.equal(doc.status, 200);
	assert.match(doc.headers["content-type"], /^text\/markdown; charset=utf-8/);
	assert.equal(doc.headers["cache-control"], "no-store");
	assert.equal(doc.headers["x-content-type-options"], "nosniff");
	assert.equal(doc.text, "# Charts [engine](../02-architecture.md)\n");
	assert.equal((await http("GET", "/api/docs/docs/lockdown/README.md")).status, 200);
	assert.equal((await http("GET", "/api/docs/docs/guide/alias.md")).text, "# RichView guide\n");
	assert.equal((await http("GET", "/api/docs/docs/guide/02%2Darchitecture.md")).status, 200);

	const refused = [
		"/api/docs/docs/lockdown/notes.md",
		"/api/docs/docs/lockdown/evidence/product.md",
		"/api/docs/docs/lockdown/parts/01-alpha.json",
		"/api/docs/docs/lockdown/answers/01-alpha.json",
		"/api/docs/docs/lockdown/RULES.md",
		"/api/docs/docs/guide/escape.md",
		"/api/docs/docs/guide/linked/x.md",
		"/api/docs/docs/guide/.draft.md",
		"/api/docs/docs/guide/notes.txt",
		"/api/docs/docs/guide/missing.md",
		"/api/docs/README.md",
		"/api/docs/outside.md",
		"/api/docs/docs/guide/../../outside.md",
		"/api/docs/docs/guide/%2e%2e/%2e%2e/outside.md",
		"/api/docs/docs/guide/..%2f..%2foutside.md",
		"/api/docs/docs/guide/..%5c..%5coutside.md",
		"/api/docs/docs%2Fguide%2FREADME.md",
		"/api/docs/docs/guide//README.md",
		"/api/docs/docs/guide/README.md%00",
		"/api/docs/docs/guide/README.md%00.md",
		"/api/docs/%2Fetc%2Fhosts",
		"/api/docs/docs/guide/components/../../../outside.md",
	];
	for (const pathname of refused) {
		const res = await http("GET", pathname);
		assert.equal(res.status, 404, pathname);
		assert.doesNotMatch(res.text, /secret/i, pathname);
	}
	assert.equal((await http("GET", "/api/docs/docs/guide/%E0%A4%A.md")).status, 400);
	assert.equal((await http("PUT", "/api/docs/docs/guide/README.md", { body: { text: "x" } })).status, 405);
	assert.equal((await http("POST", "/api/docs", { body: {} })).status, 405);
	assert.equal((await http("DELETE", "/api/docs/docs/guide/README.md")).status, 405);
	assert.equal(await readFile(path.join(root, "docs", "guide", "README.md"), "utf8"), "# RichView guide\n");
});

test("documents written or changed while the app runs show up in the next listing", async () => {
	const before = (await http("GET", "/api/docs")).json.docs;
	const oldRevision = before.find((doc) => doc.path === "docs/guide/10-later.md").revision;
	await mkdir(path.join(root, "docs", "guide", "types"), { recursive: true });
	await writeFile(path.join(root, "docs", "guide", "types", "frontmatter.md"), "# Front matter\n");
	await writeFile(path.join(root, "docs", "lockdown", "ids.md"), "# Entity IDs\n");
	await writeFile(path.join(root, "docs", "guide", "10-later.md"), "# Later, revised\n\nMore.\n");
	const after = (await http("GET", "/api/docs")).json.docs;
	const byPath = Object.fromEntries(after.map((doc) => [doc.path, doc]));
	assert.equal(byPath["docs/guide/types/frontmatter.md"].title, "Front matter");
	assert.equal(byPath["docs/lockdown/ids.md"].title, "Entity IDs");
	assert.equal(byPath["docs/guide/10-later.md"].title, "Later, revised");
	assert.notEqual(byPath["docs/guide/10-later.md"].revision, oldRevision);
	assert.deepEqual(
		after.filter((doc) => doc.section === "lockdown").map((doc) => doc.path),
		["docs/lockdown/README.md", "docs/lockdown/GOALS.md", "docs/lockdown/ids.md"],
	);
});

test("a symlinked docs/guide or docs/lockdown folder exposes nothing", async () => {
	const other = await mkdtemp(path.join(os.tmpdir(), "lockdown-app-docs-"));
	const outside = await mkdtemp(path.join(os.tmpdir(), "lockdown-app-outside-"));
	let second;
	try {
		await mkdir(path.join(other, "docs"), { recursive: true });
		await mkdir(path.join(outside, "lockdown"), { recursive: true });
		await writeFile(path.join(outside, "a.md"), "# Outside guide secret\n");
		await writeFile(path.join(outside, "lockdown", "README.md"), "# Outside lockdown secret\n");
		await symlink(outside, path.join(other, "docs", "guide"));
		await symlink(path.join(outside, "lockdown"), path.join(other, "docs", "lockdown"));
		second = await startServer(other);
		const get = (pathname) =>
			new Promise((resolve, reject) => {
				const req = request(
					{ host: "127.0.0.1", port: second.port, path: pathname, headers: { Host: `127.0.0.1:${second.port}` } },
					(res) => {
						let text = "";
						res.on("data", (chunk) => {
							text += chunk;
						});
						res.on("end", () => resolve({ status: res.statusCode, text }));
					},
				);
				req.on("error", reject);
				req.end();
			});
		const listing = await get("/api/docs");
		assert.equal(listing.status, 200);
		assert.deepEqual(JSON.parse(listing.text).docs, []);
		const repoPaths = await get("/api/repo-paths");
		assert.equal(repoPaths.status, 200);
		assert.deepEqual(JSON.parse(repoPaths.text).lockdownPaths, [], "names behind a symlinked folder stay hidden");
		for (const pathname of ["/api/docs/docs/guide/a.md", "/api/docs/docs/lockdown/README.md"]) {
			const res = await get(pathname);
			assert.equal(res.status, 404, pathname);
			assert.doesNotMatch(res.text, /secret/i, pathname);
		}
	} finally {
		second?.child.kill();
		await rm(other, { recursive: true, force: true });
		await rm(outside, { recursive: true, force: true });
	}
});

test("the item index maps every item ID to its part, read fresh from the part files", async () => {
	await writePart("10-index", 10, [item("X-01"), item("A-01")]);
	const res = await http("GET", "/api/index");
	assert.equal(res.status, 200);
	assert.equal(res.headers["cache-control"], "no-store");
	assert.equal(res.json["X-01"], "10-index");
	assert.equal(res.json["A-01"], "01-alpha", "an ID in two parts maps to the earlier part");
	assert.equal(res.json["B-02"], "02-beta");
	assert.equal(res.json["F-03"], "09-features");
	assert.equal(res.json["X-02"], undefined);

	await writePart("10-index", 10, [item("X-01"), item("X-02")]);
	const after = await http("GET", "/api/index");
	assert.equal(after.json["X-02"], "10-index", "a new item shows up without a restart");
	assert.equal(after.json["A-01"], "01-alpha");
	assert.equal((await http("PUT", "/api/index", { body: {} })).status, 405);
});

test("repo paths: without git the commit's list is empty and the lockdown folders list their files", async () => {
	await mkdir(path.join(root, "tools", "lockdown-app", "public"), { recursive: true });
	await writeFile(path.join(root, "tools", "lockdown-app", "public", "app.js"), "// app\n");
	const res = await http("GET", "/api/repo-paths");
	assert.equal(res.status, 200);
	assert.equal(res.headers["cache-control"], "no-store");
	assert.equal(res.json.commit, "b4db233");
	assert.deepEqual(res.json.paths, [], "the temporary root is not a git repository");
	const listed = new Set(res.json.lockdownPaths);
	for (const expected of [
		"docs/lockdown/parts/01-alpha.json",
		"docs/lockdown/answers/01-alpha.json",
		"docs/lockdown/screenshots/shot.png",
		"docs/guide/README.md",
		"docs/guide/components/charts.md",
		"tools/lockdown-app/public/app.js",
	]) {
		assert.ok(listed.has(expected), expected);
	}
	for (const hidden of ["docs/lockdown/parts/.03-broken.json.tmp", "docs/guide/.draft.md", "docs/guide/linked/x.md"]) {
		assert.ok(!listed.has(hidden), hidden);
	}
	assert.ok(res.json.lockdownPaths.every((entry) => /^(?:docs\/lockdown|docs\/guide|tools\/lockdown-app)\//.test(entry)));

	await writeFile(path.join(root, "docs", "guide", "new-page.md"), "# New\n");
	const after = await http("GET", "/api/repo-paths");
	assert.ok(after.json.lockdownPaths.includes("docs/guide/new-page.md"), "files on disk are listed fresh");
	assert.equal((await http("POST", "/api/repo-paths", { body: {} })).status, 405);
});

test("repo paths list the pinned commit's files from git, read once", async (t) => {
	try {
		execFileSync("git", ["--version"], { stdio: "ignore" });
	} catch {
		t.skip("git is not installed");
		return;
	}
	const repo = await mkdtemp(path.join(os.tmpdir(), "lockdown-app-git-"));
	let second;
	try {
		await mkdir(path.join(repo, ".github", "workflows"), { recursive: true });
		await mkdir(path.join(repo, "crates", "core", "src"), { recursive: true });
		await mkdir(path.join(repo, "docs", "lockdown", "parts"), { recursive: true });
		await writeFile(path.join(repo, "README.md"), "# Repo\n");
		await writeFile(path.join(repo, ".github", "workflows", "ci.yml"), "on: push\n");
		await writeFile(path.join(repo, "crates", "core", "src", "lib.rs"), "\n");
		git(repo, "init", "-q");
		git(repo, "add", "README.md", ".github", "crates");
		git(repo, "commit", "-q", "-m", "Pinned");
		// A ref named like the pinned commit stands in for it: git resolves refs before short hashes.
		git(repo, "update-ref", "refs/tags/b4db233", "HEAD");
		await writeFile(path.join(repo, "docs", "lockdown", "parts", "01-a.json"), "{}");
		second = await startServer(repo);

		const first = await http("GET", "/api/repo-paths", { at: second.port });
		assert.equal(first.status, 200);
		assert.deepEqual([...first.json.paths].sort(), [".github/workflows/ci.yml", "README.md", "crates/core/src/lib.rs"]);
		assert.deepEqual(first.json.lockdownPaths, ["docs/lockdown/parts/01-a.json"], "untracked lockdown files come from disk");

		git(repo, "update-ref", "-d", "refs/tags/b4db233");
		const again = await http("GET", "/api/repo-paths", { at: second.port });
		assert.deepEqual(again.json.paths, first.json.paths, "git ran once and its list is cached");
	} finally {
		second?.child.kill();
		await rm(repo, { recursive: true, force: true });
	}
});

test("importance: counted per part, normalized, listed across parts by /api/items and printed in the export", async () => {
	await writePart("05-rated", 5, [
		item("R-01", { importance: "key" }),
		item("R-02", { importance: " Important " }),
		item("R-03", { importance: "detail" }),
		item("R-04", { importance: "urgent" }),
		item("R-05"),
		item("R-06", { importance: "key" }),
	]);
	try {
		assert.equal((await http("PUT", "/api/answers/05-rated/R-01", { body: { decision: "accept" } })).status, 200);
		const parts = await http("GET", "/api/parts");
		const rated = parts.json.parts.find((part) => part.id === "05-rated");
		assert.deepEqual(rated.importance, {
			key: { itemCount: 2, answeredCount: 1 },
			important: { itemCount: 1, answeredCount: 0 },
			detail: { itemCount: 1, answeredCount: 0 },
			unrated: { itemCount: 2, answeredCount: 0 },
		});
		assert.deepEqual(parts.json.totals.importance.key, { itemCount: 2, answeredCount: 1 });

		// A value outside key, important and detail reads as unrated; case and spaces do not matter.
		const part = await http("GET", "/api/parts/05-rated");
		assert.deepEqual(
			part.json.items.map((entry) => entry.importance),
			["key", "important", "detail", "", "", "key"],
		);

		const keyItems = await http("GET", "/api/items?importance=key,important");
		assert.equal(keyItems.status, 200);
		assert.deepEqual(
			keyItems.json.items.map((entry) => [entry.partId, entry.id]),
			[
				["05-rated", "R-01"],
				["05-rated", "R-02"],
				["05-rated", "R-06"],
			],
		);
		assert.equal(keyItems.json.items[0].answer.decision, "accept");
		assert.equal(keyItems.json.items[1].answer, null);
		// Every part is listed with its revision, so a client sees when any of them changed.
		assert.deepEqual(
			keyItems.json.parts.map((entry) => [entry.id, entry.revision]),
			parts.json.parts.map((entry) => [entry.id, entry.revision]),
		);
		assert.ok(keyItems.json.parts.every((entry) => "revision" in entry && "error" in entry));

		const unrated = await http("GET", "/api/items?importance=unrated");
		assert.deepEqual(
			unrated.json.items.filter((entry) => entry.partId === "05-rated").map((entry) => entry.id),
			["R-04", "R-05"],
		);
		const all = await http("GET", "/api/items");
		assert.equal(all.json.items.length, parts.json.totals.itemCount);
		assert.equal((await http("GET", "/api/items?importance=urgent")).status, 400);
		assert.equal((await http("POST", "/api/items", { body: {} })).status, 405);

		const exported = await http("GET", "/api/export.md?part=05-rated");
		assert.match(exported.text, /### R-01: Title R-01\n\n- Decision: accept\n- Kind: feature; confidence: observed; importance: key\n/);
		assert.match(exported.text, /### R-05: Title R-05\n\n- Decision: open\n- Kind: feature; confidence: observed\n/);
	} finally {
		await rm(path.join(partsDir(), "05-rated.json"), { force: true });
		await rm(answersFile("05-rated"), { force: true });
	}
});

test("redaction replaces one answer and removes its old log text without touching other entries", async () => {
	await writePart("09-redact", 9, [item("R-01"), item("R-02")]);
	await http("PUT", "/api/answers/09-redact/R-01", { body: { decision: "edit", text: "private dictation", note: "private note" } });
	await http("PUT", "/api/answers/09-redact/R-02", { body: { decision: "edit", text: "keep this" } });
	const before = await readLog();
	const oldAnswerFile = await readFile(answersFile("09-redact"), "utf8");
	const body = { confirm: "redact", text: "Approved replacement" };
	assert.equal((await http("POST", "/api/redact/09-redact/R-01", { body: { text: body.text } })).status, 400);
	assert.equal((await http("POST", "/api/redact/09-redact/R-01", { body })).status, 200);
	const answer = (await readAnswers("09-redact")).answers["R-01"];
	assert.equal(answer.text, body.text);
	assert.equal(answer.note, undefined);
	const after = await readLog();
	assert.deepEqual(after.filter(x => x.partId !== "09-redact" || x.itemId !== "R-01"), before.filter(x => x.partId !== "09-redact" || x.itemId !== "R-01"));
	const target = after.filter(x => x.partId === "09-redact" && x.itemId === "R-01");
	assert.equal(target[0].text, "[redacted]");
	assert.equal(target[0].note, "[redacted]");
	assert.equal(target.at(-1).action, "redact");
	assert.ok(!JSON.stringify(after).includes("private dictation"));
	assert.ok(!JSON.stringify(after).includes("private note"));
	// Recreate the state after the log rename but before the answer rename, then retry.
	await writeFile(answersFile("09-redact"), oldAnswerFile);
	const concurrent = await Promise.all([http("POST", "/api/redact/09-redact/R-01", { body }), http("PUT", "/api/answers/09-redact/R-02", { body: { decision: "edit", text: "concurrent update" } })]);
	assert.deepEqual(concurrent.map(result => result.status), [200, 200]);
	assert.equal((await readAnswers("09-redact")).answers["R-01"].text, body.text);
	assert.ok(!JSON.stringify(await readLog()).includes("private dictation"));
	assert.equal((await readAnswers("09-redact")).answers["R-02"].text, "concurrent update");
	assert.ok((await readLog()).some(entry => entry.partId === "09-redact" && entry.itemId === "R-02" && entry.text === "concurrent update"));
	const logFile = path.join(root, "docs", "lockdown", "answers", "log.jsonl");
	const saved = await readFile(logFile, "utf8");
	await writeFile(logFile, saved + "{broken\n");
	const beforeAnswer = await readFile(answersFile("09-redact"), "utf8");
	assert.equal((await http("POST", "/api/redact/09-redact/R-01", { body })).status, 409);
	assert.equal(await readFile(answersFile("09-redact"), "utf8"), beforeAnswer);
	assert.equal(await readFile(logFile, "utf8"), saved + "{broken\n");
	await writeFile(logFile, saved);
});
