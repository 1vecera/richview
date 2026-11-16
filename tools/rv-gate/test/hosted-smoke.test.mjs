// Host reason (ARC-03): real loopback/Workerd/process controls for the local hosted smoke.
import assert from "node:assert/strict";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import { join, resolve } from "node:path";
import test from "node:test";
import { checkHidden, checkPage, localFetch, nativeServer, offlineServices, readLocal } from "../hosted-smoke.mjs";

function scratch() {
	mkdirSync("tmp", { recursive: true });
	return mkdtempSync(resolve("tmp/hosted-smoke-test-"));
}

async function listen(handler) {
	const server = http.createServer(handler);
	await new Promise(accept => server.listen(0, "127.0.0.1", accept));
	return { origin: `http://127.0.0.1:${server.address().port}`, close: () => new Promise(accept => server.close(accept)) };
}

test("local reads follow only owned origins and never send a fixture token to redirect destinations outside them", async () => {
	let escaped = 0;
	const decoy = await listen((_, response) => { escaped++; response.end("unowned"); });
	const server = await listen((request, response) => {
		if (request.url === "/remote") response.writeHead(302, { location: "https://example.invalid/" });
		else if (request.url === "/unowned") response.writeHead(302, { location: decoy.origin });
		else if (request.url === "/first") response.writeHead(302, { location: "/last" });
		else response.writeHead(200, { "content-type": "text/plain" });
		response.end("loopback");
	});
	try {
		const options = { origins: new Set([server.origin]), headers: { authorization: "Bearer fixture" }, signal: AbortSignal.timeout(5000) };
		assert.equal((await readLocal(server.origin + "/first", options)).text, "loopback");
		await assert.rejects(readLocal(server.origin + "/remote", options), /Non-loopback URL refused/);
		await assert.rejects(readLocal(server.origin + "/unowned", options), /Unowned loopback origin refused/);
		assert.throws(() => localFetch(`http://user:password@${new URL(server.origin).host}/`, options), /Non-loopback URL refused/);
		assert.equal(escaped, 0);
	} finally { await server.close(); await decoy.close(); }
});

test("the actual HTTP reader rejects private bytes, blank successful refusals, missing CSP and changed hardening", async () => {
	const server = await listen((request, response) => {
		response.setHeader("content-type", "text/html; charset=utf-8");
		response.setHeader("referrer-policy", "no-referrer");
		response.setHeader("x-content-type-options", request.url === "/sniff" ? "off" : "nosniff");
		if (request.url !== "/missing") response.setHeader("content-security-policy", request.url === "/weak" ? "default-src 'self'; base-uri 'none'" : "default-src 'none'; base-uri 'none'");
		if (request.url === "/private" || request.url === "/leak") response.statusCode = 404;
		response.end(["/public", "/weak", "/missing", "/sniff", "/leak"].includes(request.url) ? "Secret report body" : "Unavailable");
	});
	try {
		const options = { origins: new Set([server.origin]), signal: AbortSignal.timeout(5000) };
		const read = path => readLocal(server.origin + path, options);
		checkHidden(await read("/private"), ["Secret report body"]);
		assert.throws(() => checkHidden({ status: 200, url: server.origin, text: "" }, ["Secret report body"]), /Expected refusal/);
		assert.throws(() => checkHidden({ status: 404, text: '<meta property="og:image" content="secret">' }, []), /preview metadata/);
		const leaked = await read("/leak");
		assert.throws(() => checkHidden(leaked, ["Secret report body"]), /disclosed its content/);
		checkPage(await read("/public"), "Secret report body");
		for (const path of ["/weak", "/missing", "/sniff"]) {
			const page = await read(path);
			assert.throws(() => checkPage(page, "Secret report body"));
		}
	} finally { await server.close(); }
});

test("real Workerd outbound requests use only local fixture replies and unknown hosts, methods and query strings are refused", async () => {
	const { Miniflare, convertV4MiniflareOptions } = await import("miniflare");
	const root = scratch(), requests = [];
	const mf = new Miniflare(convertV4MiniflareOptions({
		script: 'export default {fetch(request) { const url=new URL(request.url); return fetch(url.searchParams.get("target"), {method:url.searchParams.get("method") || "GET"}); }};', modules: true,
		host: "127.0.0.1", port: 0, telemetry: { enabled: false }, cachePersist: join(root, "cache"),
		outboundService: offlineServices({ jwk: { kid: "fixture" }, requests }),
	}));
	try {
		const origin = await mf.ready, origins = new Set([origin.origin]);
		const send = (target, method = "GET") => localFetch(new URL(`/?${new URLSearchParams({ target, method })}`, origin), { origins, signal: AbortSignal.timeout(5000) });
		const keys = await send("https://test.cloudflareaccess.com/cdn-cgi/access/certs");
		assert.deepEqual(await keys.json(), { keys: [{ kid: "fixture" }] });
		assert.equal((await send("https://api.github.com/repos/example/publication/git/ref/heads/publish/news")).status, 200);
		for (const [url, method] of [
			["https://example.invalid/", "GET"],
			["https://test.cloudflareaccess.com/cdn-cgi/access/certs?secret=x", "GET"],
			["https://api.github.com/repos/example/publication", "POST"],
		]) assert.equal((await send(url, method)).status, 502);
		assert.deepEqual(requests.map(entry => entry.allowed), [true, true, false, false, false]);
	} finally { await mf.dispose(); rmSync(root, { recursive: true, force: true }); }
});

test("native process cleanup reaps both a started server and an aborted startup", async () => {
	const root = scratch(), owned = [];
	try {
		const ready = join(root, "ready");
		writeFileSync(ready, "#!/bin/sh\nprintf 'RichView at http://127.0.0.1:1\\nReports at http://127.0.0.1:2\\n'\nexec sleep 60\n");
		chmodSync(ready, 0o755);
		const work = join(root, "started"); mkdirSync(work);
		const evidence = {};
		const server = await nativeServer({ bin: ready, cwd: root, work, token: "fixture", signal: AbortSignal.timeout(5000), evidence });
		owned.push(evidence);
		try {
			assert.deepEqual([server.app, server.report], ["http://127.0.0.1:1", "http://127.0.0.1:2"]);
			process.kill(evidence.pid, 0);
		} finally { await server.stop(); await server.stop(); }
		assert.equal(evidence.closed, true);
		assert.throws(() => process.kill(evidence.pid, 0), { code: "ESRCH" });
		const waiting = join(root, "waiting"), second = join(root, "startup"); mkdirSync(second);
		writeFileSync(waiting, "#!/bin/sh\nexec sleep 60\n"); chmodSync(waiting, 0o755);
		const aborted = {}, controller = new AbortController(), reason = new Error("Fixture startup cancelled");
		const pending = nativeServer({ bin: waiting, cwd: root, work: second, token: "fixture", signal: controller.signal, evidence: aborted });
		owned.push(aborted);
		controller.abort(reason);
		await assert.rejects(pending, error => error === reason);
		assert.equal(aborted.closed, true);
		assert.throws(() => process.kill(aborted.pid, 0), { code: "ESRCH" });
	} finally {
		for (const evidence of owned) if (evidence.pid && !evidence.closed) {
			try { process.kill(evidence.pid, "SIGKILL"); } catch (error) { if (error.code !== "ESRCH") throw error; }
		}
		rmSync(root, { recursive: true, force: true });
	}
});
