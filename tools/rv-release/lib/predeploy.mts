// Host reason (ARC-03): the checks that run before anything is deployed (DEL-83), against the staged build and a local edge, never the live site.
import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { edgeFetch, startEdge } from "./edge.mts";
import { edgeProbes } from "./probes.mts";
import type { ReleaseDescriptor } from "./descriptor.mts";
import type { Journal } from "./journal.mts";

type RouteRow = {kind: string; ok: boolean; error?: string; route?: string; file?: string; status?: number; location?: string | null};
type Routes = {summary: {failed: RouteRow[]; checked: number; by_kind?: Record<string, unknown>}};
type WebFiles = Record<string, {bytes: number; sha256: string}>;
export type SiteChecker = {
	verifySiteBuild(input: {receipt: {commit: string; build: {snapshot: string; manifestSha256?: string}}; cwd: string; web: string}): {files: WebFiles};
	checkLiveRoutes(input: {cwd: string; web: string; webFiles: WebFiles; fetch?: typeof globalThis.fetch}): Promise<Routes>;
	verifyLiveRoutes(routes: Routes, input: {cwd: string; webFiles: WebFiles}): {checked: number};
};
type CheckResult = {status?: string; detail?: string};
type BuiltinCheck = (ctx: CheckContext) => CheckResult | Promise<CheckResult>;
export type CheckContext = {
	desc: ReleaseDescriptor; journal: Journal; repo: string; scratch: string; home?: string; newWorker?: () => string | null;
	builtin?: Record<string, BuiltinCheck>;
	deps?: {worktree?: string; kept?: string; git?: typeof git; site?: SiteChecker; wrangler?: string; startEdge?: (options: Parameters<typeof startEdge>[0]) => Promise<Pick<Awaited<ReturnType<typeof startEdge>>, "base" | "stop">>; fetch?: typeof globalThis.fetch; evidence?: boolean};
};

/** One failing row of the live comparison, in a line: what was asked, what answered and where it pointed. */
const describeRow = (row: RouteRow) => (row.kind === "error" ? `a request failed (${row.error})` : `${row.kind} ${row.route ?? row.file} answered ${row.status}${row.location ? ` to ${row.location}` : ""}`);
const git = (cwd: string, ...args: string[]) => execFileSync("git", ["-C", cwd, ...args], { encoding: "utf8" }).trim();

/**
 * The live comparison of the site, run before the deploy against the staged folder: every file bound to the full's manifest, then
 * every route, redirect and file requested from the Workers runtime serving that folder, then the read-only probes. This is the
 * check that would have refused release 31's `/docs/example/Rendered/publisher.html` (a 307 at the real edge) before the upload.
 */
export async function siteEdge({ desc, journal, repo, scratch, home = homedir(), deps = {} }: CheckContext) {
	const worktree = deps.worktree ?? join(repo, ".claude/worktrees", `site-deploy-${desc.number}`);
	const kept = deps.kept ?? join(home, "tmp/richview-rewrite/bin-release", desc.final.slice(0, 10));
	const run = deps.git ?? git;
	if (run(worktree, "rev-parse", "HEAD") !== desc.final) throw new Error(`The deploy worktree ${worktree} is not at ${desc.final}.`);
	if (run(worktree, "status", "--porcelain", "--untracked-files=normal") !== "") throw new Error(`The deploy worktree ${worktree} is not clean.`);
	const manifest = journal.get("M");
	if (!/^[0-9a-f]{64}$/.test(manifest ?? "")) throw new Error("The journal holds no build manifest digest (M); bind the full first.");
	// The candidate's own gate code, as the deployer uses it: the checks the deploy will make after the upload are the ones made here.
	const site: SiteChecker = deps.site ?? (await import(pathToFileURL(join(worktree, "tools/rv-gate/deploy-site.mjs")).href)); // Use the CLI module in the candidate checkout.
	const web = join(worktree, "dist/web");
	const verified = site.verifySiteBuild({ receipt: { commit: desc.final, build: { snapshot: kept, manifestSha256: manifest } }, cwd: worktree, web });
	mkdirSync(scratch, { recursive: true });
	const wrangler = deps.wrangler ?? join(worktree, "node_modules/.bin/wrangler");
	const edge = await (deps.startEdge ?? startEdge)({ wrangler, config: join(worktree, "wrangler.toml"), cwd: worktree, scratch });
	try {
		const fetch = edgeFetch(edge.base, deps.fetch);
		const routes = await site.checkLiveRoutes({ cwd: worktree, web, webFiles: verified.files, fetch });
		const failing = routes.summary?.failed ?? [];
		if (failing.length) throw new Error(`${failing.length} of ${routes.summary.checked} routes and files differ at the local edge: ${failing.slice(0, 5).map(describeRow).join("; ")}${failing.length > 5 ? `; and ${failing.length - 5} more` : ""}`);
		const checked = site.verifyLiveRoutes(routes, { cwd: worktree, webFiles: verified.files });
		const http = await edgeProbes({ cwd: worktree, web, fetch });
		const detail = `${checked.checked} routes and files and ${http.probes.length} probes answered by the local edge as staged; the workers.dev redirect to Cloudflare Access is checked only after the deploy`;
		if (deps.evidence !== false) writeFileSync(join(journal.here, "predeploy-site-edge.json"), `${JSON.stringify({ at: new Date().toISOString(), edge: edge.base, files: Object.keys(verified.files).length, routes: checked.checked, probes: http.probes }, null, 1)}\n`, { mode: 0o600 });
		return { status: "pass", detail };
	} finally {
		await edge.stop();
		rmSync(join(scratch, "state"), { recursive: true, force: true });
	}
}

export const BUILTIN: Record<string, BuiltinCheck> = { "site-edge": siteEdge };

/** Run one built-in check; a check that throws has failed, and its reason is kept. */
export async function runBuiltin(id: string, ctx: CheckContext) {
	const check = (ctx.builtin ?? BUILTIN)[id];
	if (!check) return { id, status: "fail", detail: `No built-in check named ${id}.` };
	try {
		const result = await check(ctx);
		return { id, status: result.status ?? "pass", detail: result.detail ?? "" };
	} catch (error) {
		return { id, status: "fail", detail: (error as Error | null)?.message ?? String(error) };
	}
}
