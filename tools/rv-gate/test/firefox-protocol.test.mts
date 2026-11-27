// Host reason (ARC-03): verify the actual source-safe diagnostic stream and its loss reporting.
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { format } from "node:util";
import { createFirefoxProtocolSink } from "../lib/firefox-protocol.mts";

const require = createRequire(import.meta.url);
const { debug } = require(join(require.resolve("playwright-core/package.json"), "../lib/utilsBundle.js"));
const sentinel = "REPORT_SOURCE_HTML_WASM_FUNCTION_ARGUMENT_RESULT_MUST_NOT_PERSIST";
const packet = (body: unknown, direction = "◀ RECV") => Buffer.from("2026-10-30T00:00:00.000Z pw:protocol " + direction + " " + JSON.stringify(body) + "\n");
const context = (sessionId: string, frameId: string) => ({ sessionId, method: "Runtime.executionContextCreated", params: { executionContextId: "ctx", auxData: { frameId, name: "" } } });

type RetainedPacket = {
	method?: string;
	session?: string;
	frame?: string;
	context?: string;
	object?: string;
	contentFrame?: string;
	ownerFrame?: string;
	completed?: boolean;
	exception?: boolean;
	url?: string;
	lifecycle?: string;
};

function fixture(t: test.TestContext) {
	mkdirSync(new URL("../../../tmp/", import.meta.url), { recursive: true });
	const dir = mkdtempSync(fileURLToPath(new URL("../../../tmp/firefox-protocol-", import.meta.url)));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const logPath = join(dir, "protocol.jsonl"), forwarded: Buffer[] = [];
	const sink = createFirefoxProtocolSink({ logPath, forward: (bytes: Buffer) => forwarded.push(Buffer.from(bytes)) });
	return { dir, logPath, sink, forwarded, records: () => readFileSync(logPath, "utf8").trim().split("\n").filter(Boolean).map((line): RetainedPacket => JSON.parse(line)) };
}

test("real installed SDK formatting is filtered across byte splits; ordinary stderr remains byte exact", t => {
	const logger = debug("pw:protocol"), lines: Buffer<ArrayBuffer>[] = [];
	logger.enabled = true;
	logger.useColors = false;
	logger.log = (...args: [unknown, ...unknown[]]) => lines.push(Buffer.from(format(...args) + "\n"));
	logger("◀ RECV " + JSON.stringify(context("page", "frame")));
	logger("SEND ► " + JSON.stringify({ id: 7, sessionId: "page", method: "Runtime.callFunction", params: { executionContextId: "ctx", functionDeclaration: sentinel, args: [sentinel] } }));
	const prefix = Buffer.from("Error: Žluťoučký 🦊 mentions pw:protocol normally\r\n\n");
	const ordinary = Buffer.concat([prefix, Buffer.from([0xff, 0xc0, 0x80]), Buffer.from("\nlast stderr has no newline")]);
	const bytes = Buffer.concat([prefix, ...lines, ordinary.subarray(prefix.length)]);
	for (const size of [1, 7, bytes.length]) {
		const f = fixture(t);
		for (let offset = 0; offset < bytes.length; offset += size) f.sink.write(bytes.subarray(offset, offset + size));
		assert.equal(f.sink.status().critical_complete, false, "an unfinished diagnostic cannot prove absence");
		const result = f.sink.end();
		assert.deepEqual(Buffer.concat(f.forwarded), ordinary);
		assert.equal(result.critical_complete, true);
		assert.equal(result.protocol.packets, 2);
		assert.deepEqual(f.sink.end(), result, "finalization is idempotent");
		assert.deepEqual(JSON.parse(readFileSync(result.metadataPath, "utf8")), result);
		assert.equal(f.records()[0].frame, "frame");
		assert.equal(readFileSync(f.logPath, "utf8").includes(sentinel), false);
	}
});

test("lifecycle and existing describeNode responses map exact sessions, contexts and iframe identities without payloads", t => {
	const f = fixture(t);
	const events: [unknown, string?][] = [
		[context("a", "frame-a")], [context("b", "frame-b")],
		[{ id: 9, sessionId: "a", method: "Runtime.evaluate", params: { executionContextId: "ctx", expression: sentinel } }, "SEND ►"],
		[{ id: 9, sessionId: "b", method: "Page.describeNode", params: { objectId: "object-b" } }, "SEND ►"],
		[{ id: 9, sessionId: "a", result: { result: { value: sentinel }, exceptionDetails: { text: sentinel } } }],
		[{ id: 9, sessionId: "b", result: { contentFrameId: "child-b", ownerFrameId: "parent-b", attributes: ["srcdoc", sentinel] } }],
		[{ sessionId: "a", method: "Runtime.executionContextDestroyed", params: { executionContextId: "ctx" } }],
		[{ sessionId: "b", method: "Runtime.executionContextDestroyed", params: { executionContextId: "ctx" } }],
		[{ method: "Page.navigationCommitted", params: { frameId: "child-b", navigationId: "nav", url: "https://user:pass@example.invalid/report?source=" + sentinel + "#" + sentinel } }],
		[{ method: "Page.eventFired", params: { frameId: "child-b", name: "load" } }],
		[{ method: "Page.navigationStarted", params: { frameId: "child-b", url: "data:text/html," + sentinel } }],
		[{ method: "Page.navigationStarted", params: { frameId: "child-b", url: "file:///private/source/report.html?source=" + sentinel } }],
		[{ method: "Page.setContent", params: { html: sentinel } }, "SEND ►"],
	];
	for (const [body, direction] of events) f.sink.write(packet(body, direction));
	const result = f.sink.end(), records = f.records();
	assert.equal(result.critical_complete, true);
	assert.deepEqual(records.filter(row => row.method === "Runtime.executionContextDestroyed").map(({ session, frame }) => ({ session, frame })), [{ session: "a", frame: "frame-a" }, { session: "b", frame: "frame-b" }]);
	assert.deepEqual(records.filter(row => row.completed).map(({ method, session, context, object, contentFrame, ownerFrame, exception }) => ({ method, session, context, object, contentFrame, ownerFrame, exception })), [
		{ method: "Runtime.evaluate", session: "a", context: "ctx", object: undefined, contentFrame: undefined, ownerFrame: undefined, exception: true },
		{ method: "Page.describeNode", session: "b", context: undefined, object: "object-b", contentFrame: "child-b", ownerFrame: "parent-b", exception: undefined },
	]);
	assert.deepEqual(records.filter(row => row.url).map(row => row.url), ["https://example.invalid/report", "data:", "file:///report.html"]);
	assert.equal(records.find(row => row.method === "Page.eventFired")?.lifecycle, "load");
	assert.equal(Buffer.concat(f.forwarded).length, 0);
	assert.equal(readFileSync(f.logPath, "utf8").includes(sentinel), false);
	assert.equal(readFileSync(f.logPath, "utf8").includes("user:pass"), false);
});

test("malformed, unsupported and non-object protocol packets never fall back to raw stderr", t => {
	for (const line of [
		"pw:protocol UNKNOWN " + sentinel,
		"\u001b[32mpw:protocol\u001b[0m wrong " + sentinel,
		" [00:00] pw:protocol malformed " + sentinel,
		"2026-10-30T00:00:00Z pw:protocol ? " + sentinel,
		'pw:protocol ◀ RECV {"method":"Runtime.executionContextCreated","params":' + sentinel,
		'pw:protocol SEND ► {"id":7,"method":"Runtime.callFunction","params":' + sentinel,
		"pw:protocol ◀ RECV null",
		"pw:protocol ◀ RECV []",
		"pw:protocol ◀ RECV {}",
		'pw:protocol ◀ RECV "' + sentinel + '"',
		'pw:protocol SEND ► {"params":{"method":"ReportSourceSecretPayload"} BROKEN',
	]) {
		const f = fixture(t);
		assert.doesNotThrow(() => f.sink.write(Buffer.from(line + "\n")), line);
		const result = f.sink.end();
		assert.equal(result.critical_complete, false, line);
		assert.equal(result.protocol.packets, 1);
		assert.equal(Buffer.concat(f.forwarded).length, 0);
		assert.equal(readFileSync(f.logPath, "utf8").includes(sentinel), false);
		assert.equal(readFileSync(f.logPath, "utf8").includes("ReportSourceSecretPayload"), false);
	}
});

test("the 4MiB retained cap reports incompleteness while ordinary trailing bytes still pass", t => {
	const f = fixture(t), event = packet(context("page", "frame"));
	for (let i = 0; i < 40000; i++) f.sink.write(event);
	f.sink.write(Buffer.from("original failure without newline"));
	const result = f.sink.end(), retained = readFileSync(f.logPath, "utf8");
	assert.equal(result.capReached, true);
	assert.equal(result.critical_complete, false);
	assert.equal(result.protocol.packets, 40000);
	assert.equal(statSync(f.logPath).size, result.bytes);
	assert.ok(result.bytes <= 4 * 1024 * 1024);
	assert.ok(result.bytes > 4 * 1024 * 1024 - 1024);
	assert.equal(retained.endsWith("\n"), true);
	assert.doesNotThrow(() => retained.trimEnd().split("\n").forEach(line => JSON.parse(line)));
	assert.equal(Buffer.concat(f.forwarded).toString(), "original failure without newline");
});

test("oversized protocol lines are discarded through newline; oversized ordinary bytes are preserved", t => {
	for (const indent of ["", " ".repeat(512)]) {
		const f = fixture(t);
		f.sink.write(Buffer.from(indent + 'pw:protocol SEND ► {"id":1,"method":"Runtime.callFunction","params":{"args":["' + sentinel));
		for (let i = 0; i < 130; i++) f.sink.write(Buffer.alloc(64 * 1024, 65));
		f.sink.write(Buffer.from('"]}}\noriginal error\n'));
		const result = f.sink.end();
		assert.equal(result.lineLimit, true);
		assert.equal(result.critical_complete, false);
		assert.equal(Buffer.concat(f.forwarded).toString(), "original error\n");
		assert.equal(readFileSync(f.logPath, "utf8").includes(sentinel), false);
	}
	const f = fixture(t), ordinary = Buffer.alloc(9 * 1024 * 1024, 66);
	for (let offset = 0; offset < ordinary.length; offset += 65536) f.sink.write(ordinary.subarray(offset, offset + 65536));
	assert.equal(f.sink.end().critical_complete, true);
	assert.deepEqual(Buffer.concat(f.forwarded), ordinary);
});

test("real open, mid-write and metadata failures remain source-safe and explicitly incomplete", t => {
	for (const failure of ["open", "write", "metadata"]) {
		const f = fixture(t);
		let sink = f.sink;
		if (failure === "open") {
			const existing = join(f.dir, "existing.jsonl");
			writeFileSync(existing, "retained prior evidence");
			sink = createFirefoxProtocolSink({ logPath: existing, forward: (bytes: Buffer) => f.forwarded.push(bytes) });
		} else if (failure === "write") {
			sink.write(packet(context("page", "frame")));
			rmSync(f.logPath);
			mkdirSync(f.logPath);
		} else mkdirSync(f.logPath + ".meta.json");
		assert.doesNotThrow(() => sink.write(packet({ id: 1, method: "Runtime.callFunction", params: { functionDeclaration: sentinel } }, "SEND ►")));
		sink.write(Buffer.from("live original error\n"));
		const result = sink.end();
		assert.equal(result.critical_complete, false);
		assert.equal(result.errors.length, 1);
		assert.equal(result.errors[0].operation, failure);
		assert.equal(result[failure === "metadata" ? "metadataWriteLost" : "writeLost"], true);
		assert.equal(JSON.stringify(result).includes(sentinel), false);
		assert.equal(Buffer.concat(f.forwarded).toString(), "live original error\n");
		if (failure === "open") assert.equal(readFileSync(join(f.dir, "existing.jsonl"), "utf8"), "retained prior evidence");
	}
});
