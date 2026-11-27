// Deploy https://richview.ai (Worker richview-teaser) from a candidate that a complete full passed,
// the way the 10 and 12 Nov deploys were done by hand: the full's kept build, checked file by file
// against its manifest, in a locked worktree at the candidate commit, with the pinned Wrangler.
// The gate's own tools/rv-gate/deploy-site.mjs accepts only hotfix receipts; this wrapper reuses its
// build verification and live checks (imported from the candidate's own copy) for a full receipt.
//
//   node deploy-site-kept.mjs --candidate candidate/17 --receipt full-<id> --manifest-sha256 <64 hex>
//       dry run: everything up to `wrangler deploy --dry-run`; nothing changes on Cloudflare. It also
//       runs before the candidate is cut, and says so.
//   node deploy-site-kept.mjs --candidate candidate/17 --receipt full-<id> --manifest-sha256 <64 hex> --go "<Daniel's words and date>"
//       deploys, then checks every route and file on the live site. Refuses unless the candidate is cut
//       at the receipt's commit and prod is the candidate.
//
// --manifest-sha256 is the hash of the full's build manifest, written on the gos page when the full's
// build was kept (promote-full.mjs prints it after checking the kept outputs against the run's manifest).
// The gate deletes a full's run folder when a newer full passes, so the kept build's manifest is checked
// against this hash, and against the run folder's copy only while that still exists.
//
// Uses Wrangler's stored login on Claw Mini; no secret is read, printed or written.
import {arm,priorFor} from './worker-history.mts';
import type { Deployment } from './worker-history.mts';
import type { SiteChecker } from '../lib/predeploy.mts';
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, lstatSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const repo = process.env.RV_REPO, home = process.env.HOME;
if (!repo) throw Error("Run through the release command: RV_REPO names the repository.");
const store = `${home}/.local/share/richview-gate/evidence`;
const WRANGLER = "4.154.0", VERSION = /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
const sha = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");
function insist(ok: unknown, message: string): asserts ok { if (!ok) throw new Error(message); }
const options: Record<string, string> = {};
for (let i = 2; i < process.argv.length; i += 2) {
	const key = process.argv[i];
	insist(["--candidate", "--receipt", "--manifest-sha256", "--go"].includes(key) && process.argv[i + 1] !== undefined && !options[key.slice(2)], "usage: deploy-site-kept.mjs --candidate candidate/<n> --receipt full-<id> --manifest-sha256 <64 hex> [--go '<Daniel words and date>']");
	options[key.slice(2)] = process.argv[i + 1];
}
const { candidate, receipt: receiptId, go } = options, expectedManifest = options["manifest-sha256"];
insist(/^candidate\/[1-9]\d*$/.test(candidate ?? "") && /^full-\d{8}T\d{6}Z-[0-9a-f]{10}$/.test(receiptId ?? ""), "a candidate and a full receipt id are required");
insist(/^[0-9a-f]{64}$/.test(expectedManifest ?? ""), "--manifest-sha256 needs the full's build manifest hash from the gos page");
insist(go === undefined || go.replace(/\s/g, "").length >= 8, "--go needs Daniel's words and date");
const live = go !== undefined;
const n = candidate.split("/")[1], out = process.env.RV_SITE_OUT ?? `${repo}/tmp/team/launch/restage/site-deploy-${n}`;
mkdirSync(out, { recursive: true });
const run = (name: string, program: string, args: string[], cwd: string) => {
	const result = spawnSync(program, args, { cwd, encoding: "utf8", timeout: 15 * 60_000, maxBuffer: 64 * 1024 * 1024, env: { ...process.env, CI: "true", WRANGLER_SEND_METRICS: "false", CLOUDFLARE_SEND_METRICS: "false" } });
	writeFileSync(join(out, `${live ? "go" : "dry"}-${name}.log`), `${program} ${args.join(" ")}\nexit ${result.status}\n${result.stdout ?? ""}\n${result.stderr ?? ""}`);
	insist(result.status === 0, `${name} failed (${result.status}); see ${out}`);
	return result.stdout.trim();
};
const git = (name: string, ...args: string[]) => run(name, "git", args, repo);

// The receipt, the candidate and prod.
const receipt = JSON.parse(readFileSync(`${store}/receipts/${receiptId}.json`, "utf8")) as {result: string; commit: string; tree: string};
insist(receipt.result === "pass" && /^[0-9a-f]{40}$/.test(receipt.commit) && receiptId.endsWith(receipt.commit.slice(0, 10)), "the receipt is not a passing full of one commit");
git("fetch", "fetch", "-q", "origin");
const remote = Object.fromEntries(git("ls-remote", "ls-remote", "--heads", "origin", candidate, "prod").split("\n").map(line => line.split("\t").reverse()));
const cut = remote[`refs/heads/${candidate}`], candidateCut = cut === receipt.commit;
insist(candidateCut || (!live && cut === undefined), cut === undefined ? `${candidate} is not cut yet` : `${candidate} is not the receipt's commit`);
const prodMoved = remote["refs/heads/prod"] === receipt.commit;
insist(!live || prodMoved, "prod is not the candidate yet: the prod merge comes first");
insist(git("tree", "rev-parse", `${receipt.commit}^{tree}`) === receipt.tree, "the receipt's tree differs from the commit's");

// The kept build is the one the full tested.
const kept = `${home}/tmp/richview-rewrite/bin-release/${receipt.commit.slice(0, 10)}`;
const manifestSha256 = sha(readFileSync(`${kept}/build-manifest.json`));
insist(manifestSha256 === expectedManifest, "the kept build's manifest differs from --manifest-sha256");
const runManifest = `${store}/runs/${receiptId}/build-manifest.json`;
insist(!existsSync(runManifest) || manifestSha256 === sha(readFileSync(runManifest)), "the kept build's manifest differs from the full's");

// A locked worktree at the candidate commit, with the pinned Wrangler.
const cwd = `${repo}/.claude/worktrees/site-deploy-${n}`;
if (!existsSync(cwd)) {
	git("worktree-add", "worktree", "add", "-q", "--detach", cwd, receipt.commit);
	git("worktree-lock", "worktree", "lock", "--reason", "active stream", cwd);
}
insist(run("head", "git", ["rev-parse", "HEAD"], cwd) === receipt.commit && run("status", "git", ["status", "--porcelain", "--untracked-files=no"], cwd) === "", "the deploy worktree is not the clean candidate");
if (!existsSync(join(cwd, "node_modules/wrangler/package.json"))) run("npm-ci", "npm", ["ci", "--ignore-scripts"], cwd);
const wrangler = join(cwd, "node_modules/.bin/wrangler"), config = join(cwd, "wrangler.toml");
insist(JSON.parse(readFileSync(join(cwd, "node_modules/wrangler/package.json"), "utf8")).version === WRANGLER && run("wrangler-version", wrangler, ["--version"], cwd).split(/\s/)[0] === WRANGLER, `Wrangler ${WRANGLER} is required`);
const site: SiteChecker & {checkLiveHttp(options: {cwd: string; web: string}): Promise<{status: string}>} = await import(pathToFileURL(join(cwd, "tools/rv-gate/deploy-site.mjs")).href);

// Stage the kept build's site and bind every byte to the manifest.
const web = join(cwd, "dist/web"), bound = { commit: receipt.commit, build: { snapshot: kept, manifestSha256 } };
if (existsSync(join(cwd, "dist"))) { insist(lstatSync(join(cwd, "dist")).isDirectory(), "dist must be a folder of this worktree"); rmSync(join(cwd, "dist"), { recursive: true }); }
mkdirSync(join(cwd, "dist"));
cpSync(join(kept, "dist/web"), web, { recursive: true, dereference: false });
const verified = site.verifySiteBuild({ receipt: bound, cwd, web });
const wr = (name: string, args: string[]) => run(name, wrangler, [...args, "--config", config], cwd);
const current = (text: string) => {
	const row = (JSON.parse(text) as Deployment[]).toSorted((a, b) => Date.parse(a.created_on) - Date.parse(b.created_on)).at(-1);
	insist(row?.versions?.length === 1 && row.versions[0].percentage === 100 && VERSION.test(row.versions[0].version_id), "the Worker does not have one current version at 100 %");
	return { version: row.versions[0].version_id, created: row.created_on, message: row.annotations?.["workers/message"] ?? "" };
};
const before = current(wr("deployments-before", ["deployments", "list", "--json"]));
const rollback = `${wrangler} rollback ${before.version} --config ${config} --message "back to the version before ${candidate}" --yes`;
type LiveAttempt = {attempt: number; at: string; result: string; error?: string};
const report = { candidate, commit: receipt.commit, receipt: receiptId, kept, manifestSha256, files: Object.keys(verified.files).length, configSha256: sha(readFileSync(config)), wrangler: WRANGLER, before, rollback, candidateCut, prodMoved, at: new Date().toISOString() } as {
	candidate: string; commit: string; receipt: string; kept: string; manifestSha256: string; files: number; configSha256: string; wrangler: string;
	before: ReturnType<typeof current>; rollback: string; candidateCut: boolean; prodMoved: boolean; at: string;
	liveToday?: {checked: number; differing: number; by_kind?: Record<string, unknown>}; go?: string; version?: string;
	liveChecks?: LiveAttempt[]; routes?: number; http?: string; result?: string; error?: string; finished?: string;
};

if (!live) {
	wr("deploy-dry-run", ["deploy", "--keep-vars", "--dry-run"]);
	// How the live site differs from this build today, for the row on the gos page; not a pass or fail.
	const routes = await site.checkLiveRoutes({ cwd, web, webFiles: verified.files });
	report.liveToday = { checked: routes.summary.checked, differing: routes.summary.failed.length, by_kind: routes.summary.by_kind };
	writeFileSync(join(out, "dry-live-routes.json"), JSON.stringify(routes, null, 1));
	writeFileSync(join(out, "dry-run.json"), JSON.stringify(report, null, 1));
	console.log(`DRY RUN PASS ${candidate} ${receipt.commit.slice(0, 10)}: ${report.files} files match the full's manifest; Wrangler ${WRANGLER} dry run ok; live is ${before.version} (${before.message.slice(0, 60)}); ${report.liveToday.differing} of ${report.liveToday.checked} live routes and files differ from this build; ${candidateCut ? "" : `${candidate} is NOT cut yet; `}prod ${prodMoved ? "is" : "is NOT yet"} the candidate. Nothing deployed.`);
} else {
	insist(before.version === priorFor(receipt.commit), "Worker differs from the reviewed predecessor");
	report.go = go.trim();
	// "wx": a second --go must not overwrite the saved rollback with the version it deployed itself.
	writeFileSync(join(out, "rollback.json"), JSON.stringify({ before, rollback }, null, 1), { flag: "wx" });
	insist(current(wr("deployments-confirm", ["deployments", "list", "--json"])).version === before.version, "the Worker changed since the rollback version was recorded");
	const message = arm(wrangler,config,process.env.RV_SITE_JOURNAL!,{sha:receipt.commit,id:receiptId,candidate});
	try {
		// Inside the try: Wrangler can fail after the new version is already live.
		const deployed = wr("deploy", ["deploy", "--keep-vars", "--message", message]).match(/Current Version ID:\s*([0-9a-f-]{36})/);
		insist(deployed && VERSION.test(deployed[1]) && deployed[1] !== before.version, "the deploy did not name a new Worker version");
		report.version = deployed[1];
		insist(current(wr("deployments-after", ["deployments", "list", "--json"])).version === report.version, "the live deployment is not the new version");
		// Release 31 (Daniel, 25 Nov 2026, board ask deploy-check-propagation-wait: "Yes, wait up to 3 minutes, then the same full comparison").
		// On 24 Nov the edge still served the previous bytes for 106 of 1,172 routes a few seconds after the upload. The comparison below is the
		// gate's own, unchanged: every route and file against the staged build, then the HTTP and header probes. The only difference is that a
		// failing pass is repeated every 15 s until 3 minutes have passed since the deploy returned. Each attempt is kept; a pass on any attempt
		// is a pass of the full comparison, and a failure on the last attempt is the failure of that comparison.
		const settleStart = Date.now();
		report.liveChecks = [];
		for (let attempt = 1; ; attempt++) {
			let failure;
			try {
				const routes = await site.checkLiveRoutes({ cwd, web, webFiles: verified.files });
				writeFileSync(join(out, `live-routes-attempt-${attempt}.json`), JSON.stringify(routes, null, 1));
				report.routes = site.verifyLiveRoutes(routes, { cwd, webFiles: verified.files }).checked;
				report.http = (await site.checkLiveHttp({ cwd, web })).status;
				writeFileSync(join(out, "live-routes.json"), JSON.stringify(routes, null, 1));
				report.liveChecks.push({ attempt, at: new Date().toISOString(), result: "pass" });
				break;
			} catch (error) { failure = error; }
			report.liveChecks.push({ attempt, at: new Date().toISOString(), result: "fail", error: String((failure as Error | null)?.message ?? failure) });
			if (Date.now() - settleStart >= 180_000) throw failure;
			await new Promise(resolve => setTimeout(resolve, 15_000));
		}
		report.result = "pass";
		console.log(`DEPLOYED ${candidate} ${receipt.commit.slice(0, 10)} as ${report.version}: ${report.routes} live routes and files checked, 0 failures. For recovery inspect the attempt with recover-worker-history.`);
	} catch (error) {
		report.result = "fail"; report.error = String((error as Error | null)?.message ?? error);
		console.error(`THE DEPLOY OR A LIVE CHECK FAILED: ${report.error}\nThe new version may be live: read ${out}/go-deploy.log and the Worker's deployment list first. Use bash steps/run recover-worker-history before choosing any recovery.`);
		process.exitCode = 1;
	} finally { report.finished = new Date().toISOString(); writeFileSync(join(out, "deploy.json"), JSON.stringify(report, null, 1)); }
}
