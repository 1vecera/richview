// Host reason (ARC-03): the read-only HTTP probes of the gate's live check, run against the local edge before a deploy (DEL-83).
// They mirror checkLiveHttp in tools/rv-gate/deploy-site.mjs without its last probe, the workers.dev redirect to Cloudflare Access,
// which only the real account can answer: that probe stays in the checks after the deploy.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { LIVE_BASE } from "./edge.mts";

export const INSTALL_COMMAND = "curl -fsSL https://github.com/1vecera/richview/releases/latest/download/install.sh | sh";
const sha = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");
const json = (file: string) => JSON.parse(readFileSync(file, "utf8")) as Record<string, unknown>;
const insist = (ok: unknown, message: string) => { if (!ok) throw new Error(message); };

/** The headers `_headers` gives one path (the `/*` block and an exact block), as the gate's staged-header reader does. */
export function stagedHeaders(web: string, path: string): Record<string, string> {
	const blocks: {path: string; headers: Record<string, string>}[] = [];
	let block: (typeof blocks)[number] | undefined;
	for (const line of readFileSync(join(web, "_headers"), "utf8").split(/\r?\n/)) {
		if (!line.trim() || line.trimStart().startsWith("#")) continue;
		if (!/^\s/.test(line)) { block = { path: line.trim(), headers: {} }; blocks.push(block); }
		else {
			const match = line.trim().match(/^([^:]+):\s*(.*)$/);
			if (match && block) block.headers[match[1].toLowerCase()] = match[2];
		}
	}
	return Object.assign({}, ...blocks.filter((b) => b.path === "/*" || b.path === path).map((b) => b.headers));
}

/** Probe the install page, the retired installer routes, the interest endpoint and the security headers. */
export async function edgeProbes({ cwd, web, fetch }: {cwd: string; web: string; fetch: typeof globalThis.fetch}) {
	const get = async (path: string) => {
		const response = await fetch(LIVE_BASE + path, { redirect: "manual", headers: { "Accept-Encoding": "identity", "User-Agent": "richview-edge-check/1" }, signal: AbortSignal.timeout(30_000) });
		const body = Buffer.from(await response.arrayBuffer());
		return { path, status: response.status, headers: Object.fromEntries(response.headers.entries()), sha256: sha(body), body: body.toString("utf8") };
	};
	const probes = [];
	const install = json(join(cwd, "assets/routes.json")).install;
	insist(Array.isArray(install) && JSON.stringify(install) === JSON.stringify(["/install"]), "Install route probe list must contain /install only");
	const enabled = json(join(cwd, "assets/install/release.json")).enabled;
	insist(typeof enabled === "boolean", "Install release switch is missing");
	const page = await get("/install");
	if (enabled) insist(page.status === 200 && page.sha256 === sha(readFileSync(join(web, "install.html"))) && page.body.includes(INSTALL_COMMAND), "The install page at the edge must match the staged page and show the GitHub release command");
	else insist(page.status === 404, `Install route must remain 404 at the edge (${page.status})`);
	probes.push({ path: page.path, status: page.status });
	for (const path of ["/install.sh", "/install-release.json", "/downloads/"]) {
		const old = await get(path);
		insist(old.status === 404, `Retired installer route must be 404 at the edge: ${path} (${old.status})`);
		probes.push({ path, status: old.status });
	}
	const interest = await get("/api/interest");
	insist(interest.status === 405 && interest.headers.allow === "POST", "GET /api/interest must return 405 with Allow: POST at the edge");
	probes.push({ path: interest.path, status: interest.status });
	for (const path of ["/", "/richview_runtime.js", "/richview_runtime_bg.wasm"]) {
		const row = await get(path);
		insist(row.status === 200, `Header probe did not serve ${path} at the edge (${row.status})`);
		const expected = stagedHeaders(web, path);
		insist(expected["x-content-type-options"] === "nosniff" && expected["referrer-policy"], "Staged security headers are incomplete");
		if (path === "/") insist(expected["content-security-policy"], "Staged home CSP is missing");
		for (const [name, value] of Object.entries(expected)) insist(row.headers[name] === value, `Header differs at the edge on ${path}: ${name}`);
		probes.push({ path, status: row.status });
	}
	return { status: "pass", probes };
}
