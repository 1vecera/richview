// Host reason (ARC-03): serves the staged site through the Workers runtime on loopback, so the live comparison can run before a deploy (DEL-83).
// `wrangler dev --local` runs Cloudflare's own asset handling (html_handling, _redirects, _headers) with no account, token or network:
// a route the real edge redirects, such as `/page.html`, is redirected here too.
import { spawn as nodeSpawn } from "node:child_process";
import { closeSync, mkdirSync, openSync, readFileSync } from "node:fs";
import { createServer } from "node:net";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

import type { AddressInfo } from "node:net";

type EdgeOptions = {wrangler: string; config: string; cwd: string; scratch: string; port?: number; spawn?: typeof nodeSpawn; fetch?: typeof globalThis.fetch; timeoutMs?: number; pollMs?: number; env?: NodeJS.ProcessEnv};

export const LIVE_BASE = "https://richview.ai";

const freePort = () => new Promise<number>((resolve, reject) => {
	const server = createServer();
	server.once("error", reject);
	server.listen(0, "127.0.0.1", () => { const { port } = server.address() as AddressInfo; server.close(() => resolve(port)); });
});

/** The environment a local edge gets: no Cloudflare credential, no home directory with a login, no metrics. */
export function edgeEnvironment(scratch: string, base = process.env) {
	const home = join(scratch, "home");
	mkdirSync(join(home, ".config"), { recursive: true });
	return { PATH: base.PATH ?? "", HOME: home, XDG_CONFIG_HOME: join(home, ".config"), TMPDIR: base.TMPDIR ?? "/tmp", WRANGLER_SEND_METRICS: "false", CLOUDFLARE_SEND_METRICS: "false", CI: "true", NO_COLOR: "1" };
}

/**
 * Start the edge for a staged deploy folder and wait until it answers. The child is the only process this stops, by its own PID.
 * `spawn` and `ready` are injectable so a test needs no Wrangler.
 */
export async function startEdge({ wrangler, config, cwd, scratch, port, spawn = nodeSpawn, fetch = globalThis.fetch, timeoutMs = 180_000, pollMs = 500, env }: EdgeOptions) {
	mkdirSync(scratch, { recursive: true });
	port ??= await freePort();
	const base = `http://127.0.0.1:${port}`, logFile = join(scratch, "edge.log"), log = openSync(logFile, "w", 0o600);
	const child = spawn(wrangler, ["dev", "--config", config, "--ip", "127.0.0.1", "--port", String(port), "--local", "--persist-to", join(scratch, "state")], { cwd, env: env ?? edgeEnvironment(scratch), stdio: ["ignore", log, log] });
	closeSync(log);
	let exited = false, exitCode;
	child.once("exit", (code) => { exited = true; exitCode = code; });
	const tail = () => { try { return readFileSync(logFile, "utf8").split("\n").slice(-15).join("\n"); } catch { return ""; } };
	const stop = async () => {
		if (exited) return;
		child.kill("SIGTERM");
		for (let i = 0; i < 40 && !exited; i++) await sleep(250);
		if (!exited) { child.kill("SIGKILL"); for (let i = 0; i < 20 && !exited; i++) await sleep(100); }
	};
	const started = Date.now();
	for (;;) {
		if (exited) throw new Error(`The local edge stopped before it answered (exit ${exitCode}):\n${tail()}`);
		try { await fetch(`${base}/`, { redirect: "manual", signal: AbortSignal.timeout(5_000) }); break; } catch { /* not listening yet */ }
		if (Date.now() - started > timeoutMs) { await stop(); throw new Error(`The local edge did not answer within ${Math.round(timeoutMs / 1000)} s:\n${tail()}`); }
		await sleep(pollMs);
	}
	return { base, port, pid: child.pid, logFile, stop };
}

/** A fetch that answers `https://richview.ai/...` from the local edge and refuses every other address. */
export function edgeFetch(base: string, fetchImpl = globalThis.fetch): typeof globalThis.fetch {
	return async (url, init) => {
		const target = String(url);
		if (!target.startsWith(LIVE_BASE)) throw new Error(`The edge check makes no request outside the local edge: ${target}`);
		const response = await fetchImpl(base + target.slice(LIVE_BASE.length), init);
		const location = response.headers.get("location");
		if (!location?.startsWith(base)) return response;
		// A redirect names the host the request went to; the live comparison expects the live host.
		const headers = new Headers(response.headers);
		headers.set("location", LIVE_BASE + location.slice(base.length));
		return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
	};
}
