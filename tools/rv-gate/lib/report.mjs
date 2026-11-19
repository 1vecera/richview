// The before-and-after report (DEL-69), the go card and packet (DEL-34, DEL-71) and the one
// command that merges a frozen candidate into prod on Daniel's go (DEL-04).

import { copyFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { basename, isAbsolute, join } from "node:path";
import { patchId } from "./fastpass.mjs";
import { backmergeHotfix, hotfixFailureError } from "./hotfix-backmerge.mjs";
import { hotfixSteps } from "./hotfix.mjs";
import {affectedPromotion} from "./affected-promotion.mjs";
import { loadRetryLine } from "./perf.mjs";
import { previewReloadLine } from "./preview-reload.mjs";
import { gateEnv, notRunParts, sh } from "./proc.mjs";
import { acquireLock, capturePath, evidenceDir, listReceipts, readChanges, readReceipt, receiptNotes, writeReceiptNote } from "./store.mjs";
import { WINDOWS_NOTE, readWindowsRun, shipsWindows, windowsMergeProblem } from "./windows.mjs";

const esc = (value) =>
	String(value ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

function isAncestor(cwd, a, b) {
	return sh("git", ["merge-base", "--is-ancestor", a, b], { cwd, allowFail: true }).status === 0;
}

/** Change-log entries merged into `to` and not into `from`, in merge order, with reverts linked. */
export function changesInRange({ cwd, from, to }) {
	const entries = readChanges().filter((entry) => isAncestor(cwd, entry.commit, to) && !isAncestor(cwd, entry.commit, from));
	for (const entry of entries) {
		const revert = entries.find((other) => other !== entry && other.title?.includes(entry.commit.slice(0, 7)) && /^Revert/i.test(other.title));
		if (revert) entry.revertedBy = revert.commit;
		entry.verdicts = receiptNotes(entry.receipt).filter((note) => note.kind === "design-verdict");
	}
	return entries;
}

function groupKey(name) {
	const parts = name.split("/");
	if (parts[0] === "fixture" || parts[0] === "site") return `${parts[0]} · ${parts[1]}`;
	return parts.slice(0, 2).join(" · ");
}

function copyCapture(hash, assets) {
	if (!hash) return undefined;
	const file = `${hash}.png`;
	const target = join(assets, file);
	if (!existsSync(target) && existsSync(capturePath(hash))) copyFileSync(capturePath(hash), target);
	return `assets/${file}`;
}

function figure(label, src) {
	return src ? `<figure><img loading="lazy" src="${esc(src)}" alt="${esc(label)}"><figcaption>${esc(label)}</figcaption></figure>` : "";
}

/** Pick the same exact-head evidence that merge-prod will consider for a candidate. */
function candidateReceipt(receipts, head, candidate) {
	return receipts.filter(r => r.commit === head && ((["prod", "hotfix"].includes(r.kind) && r.candidate === candidate && candidate) || ["full", "affected"].includes(r.kind))).at(-1);
}

function evidenceScope(receipt, head, candidate) {
	if (!receipt) return `<p class="missing">No candidate receipt is recorded for this exact source. The production merge cannot use this packet as evidence.</p>`;
	const promotion = receiptNotes(receipt.id).find(note => note.kind === "promotion" && note.commit === head && note.tree === receipt.tree);
	const chromeOnly = ["full", "affected"].includes(receipt.kind) && receipt.phase === "prelaunch" && JSON.stringify(receipt.phasePolicy?.prelaunchFull?.browsers) === JSON.stringify(["chromium"]);
	const browser = chromeOnly ? "Chromium only: Firefox and WebKit were skipped under the prelaunch policy." : receipt.kind === "hotfix" ? "Hotfix browser coverage is limited to affected Chromium checks; see the recorded steps." : "See the recorded results below for browser coverage.";
	const row = (status, id, reason) => `<tr><td>${esc(status)}</td><td><code>${esc(id)}</code></td><td>${esc(reason ?? "")}</td></tr>`;
	const selectionRows = (source, current = false) => (source.selection ?? []).map(item => {
		const passedEarlier = current && item.status === "skipped" && item.decisions?.some(decision => decision.status === "run")
			&& promotion?.affectedProof?.outcomes?.findLast(entry => ["pass", "fail"].includes(entry.outcomes?.[item.id]))?.outcomes?.[item.id] === "pass";
		const status = item.status === "run" ? `run: ${source.outcomes?.[item.id] ?? "unrecorded"}` : passedEarlier ? "run: pass (earlier run)" : item.status;
		const label = item.caseKey ? `${item.command ?? item.commandId}: ${item.caseKey} (${item.id})` : item.id;
		return row(status, label, [item.reason, ...(item.decisions ?? []).filter(decision => decision.status !== "skipped").map(decision => `${decision.commit.slice(0,10)}: ${decision.status} — ${decision.reason ?? ""}`)].filter(Boolean).join("; "));
	}).join("");
	const stepRows = source => (source.steps ?? []).map(step => row(step.status, step.id, step.reason) + notRunParts(step).map(part => row("unrun", `${step.id}: ${part.what}`, part.reason)).join("")).join("");
	let history = "";
	if (receipt.kind === "affected" && Array.isArray(promotion?.affectedProof?.receipts)) {
		const earlier = promotion.affectedProof.receipts.filter(id => id !== receipt.id);
		if (earlier.length) history = `<h3>Earlier affected runs used for coverage</h3>${earlier.map(id => {
			try { const prior = readReceipt(id); return `<details><summary>${esc(id)} — ${esc(prior.result)}</summary><table><thead><tr><th>Result</th><th>Unit</th><th>Selection reason</th></tr></thead><tbody>${selectionRows(prior)}</tbody></table></details>`; }
			catch { return `<p class="missing">Earlier receipt ${esc(id)} is unavailable.</p>`; }
		}).join("")}`;
	}
	const rows = receipt.kind === "affected" ? selectionRows(receipt, true) : stepRows(receipt);
	const promotionText = promotion ? "Promoted to pre-prod for this source." : candidate && ["prod", "hotfix"].includes(receipt.kind) ? "Candidate-specific receipt." : "No matching pre-prod promotion note is recorded.";
	const missingDecisions = receipt.kind === "affected" && receipt.obligations?.length && !receipt.selection?.some(item => Array.isArray(item.decisions));
	const warning = missingDecisions ? `<p class="missing">This earlier receipt did not preserve per-commit pause reasons; its skipped labels may include paused or previously covered units.</p>` : "";
	return `<p>Receipt <code>${esc(receipt.id)}</code> (${esc(receipt.kind)}, ${esc(receipt.result)}). ${esc(promotionText)} ${esc(browser)}</p>${warning}<table><thead><tr><th>Result</th><th>Unit</th><th>Reason</th></tr></thead><tbody>${rows || '<tr><td colspan="3">No unit details recorded.</td></tr>'}</tbody></table>${history}`;
}

/** Render the report into the evidence store and return its index path. */
export async function writeReport({ cwd, from = "origin/prod", to = "origin/dev", title } = {}) {
	sh("git", ["fetch", "-q", "origin"], { cwd });
	const fromSha = sh("git", ["rev-parse", from], { cwd });
	const toRef = /^candidate\/\d+$/.test(to) ? `origin/${to}` : to;
	const toSha = sh("git", ["rev-parse", toRef], { cwd });
	const entries = changesInRange({ cwd, from: fromSha, to: toSha });
	const candidate = /^(?:origin\/)?(candidate\/\d+)$/.exec(to)?.[1];
	const receipts = listReceipts();
	const receipt = candidateReceipt(receipts, toSha, candidate);
	const candidateEvidence = !candidate && receipts.some(row => row.commit === toSha && ["prod", "hotfix"].includes(row.kind) && row.candidate);
	const evidence = (candidateEvidence ? `<p class="missing">Candidate-specific receipts also exist at this head. Generate the packet for the exact candidate to see the receipt merge-prod will select.</p>` : "") + evidenceScope(receipt, toSha, candidate);
	const stamp = new Date().toISOString().replace(/[-:]/g, "").replace(/\.\d+Z$/, "Z");
	const dir = join(evidenceDir(), "reports", `${stamp}-${fromSha.slice(0, 8)}..${toSha.slice(0, 8)}`);
	const assets = join(dir, "assets");
	mkdirSync(assets, { recursive: true });

	const design = entries.filter((e) => e.class === "design");
	const feature = entries.filter((e) => e.class === "feature");
	const none = entries.filter((e) => e.class === "none");

	// Together: the net contact sheet, the latest image of every capture changed in the range.
	const net = new Map();
	for (const entry of entries) for (const capture of entry.changedCaptures ?? []) net.set(capture.name, { ...net.get(capture.name), name: capture.name, after: capture.after, before: net.get(capture.name)?.before ?? capture.before });
	const groups = new Map();
	for (const capture of net.values()) {
		const key = groupKey(capture.name);
		if (!groups.has(key)) groups.set(key, []);
		groups.get(key).push(capture);
	}
	const sheet = [...groups]
		.sort(([a], [b]) => a.localeCompare(b))
		.map(([key, captures]) => `<section class="group"><h3>${esc(key)}</h3><div class="grid">${captures.map((c) => `<div class="pair">${figure(`${c.name} · before`, copyCapture(c.before, assets))}${figure(`${c.name} · after`, copyCapture(c.after, assets))}</div>`).join("")}</div></section>`)
		.join("");

	const entryHtml = (entry) => {
		const cause = entry.cause ? `${entry.cause.kind}${entry.cause.detail ? `: ${entry.cause.detail}` : ""}` : "not declared";
		const captures = (entry.changedCaptures ?? [])
			.filter((c) => !/\/(?:768|320)$/.test(c.name))
			.map((c) => `<div class="pair">${figure(`${c.name} · before`, copyCapture(c.before, assets))}${figure(`${c.name} · after`, copyCapture(c.after, assets))}</div>`)
			.join("");
		const verdicts = entry.verdicts?.length
			? entry.verdicts.map((v) => `<p class="verdict"><b>Review (${esc(v.model ?? "agent")}):</b> ${esc(v.verdict)}</p>`).join("")
			: entry.class === "design"
				? `<p class="missing">No review verdict recorded yet.</p>`
				: "";
		const surfaces = entry.changedSurfaces?.length ? `<p><b>Public surface:</b> ${entry.changedSurfaces.map(esc).join(", ")}</p>` : "";
		const deleted = entry.deletedTests?.length ? `<p><b>Deleted tests:</b></p><ul>${entry.deletedTests.map((t) => `<li><code>${esc(t.test)}</code>: ${esc(t.reason)}</li>`).join("")}</ul>` : "";
		return `<article class="entry ${esc(entry.class)}${entry.revertedBy ? " reverted" : ""}" id="pr-${esc(entry.pr)}">
<h3>#${esc(entry.pr)} ${esc(entry.title)}</h3>
<dl><dt>Cause</dt><dd>${esc(cause)}</dd><dt>Items</dt><dd>${esc((entry.items ?? []).join(", ") || "none named")}</dd>
<dt>Assumed</dt><dd>${esc((entry.assumed ?? []).join(", ") || "none")}</dd>
<dt>Issue</dt><dd>${esc(entry.issue)}</dd><dt>Receipt</dt><dd><code>${esc(entry.receipt)}</code></dd>
<dt>Merged</dt><dd><code>${esc(entry.commit.slice(0, 10))}</code> ${esc(entry.at)}</dd><dt>Revert</dt><dd><code>${esc(entry.revert)}</code></dd></dl>
${entry.revertedBy ? `<p class="missing">Reverted by ${esc(entry.revertedBy.slice(0, 10))}.</p>` : ""}${verdicts}${surfaces}${deleted}<div class="grid">${captures}</div></article>`;
	};

	const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="robots" content="noindex"><title>${esc(title ?? "RichView before and after")}</title>
<style>
body{font:15px/1.5 system-ui,sans-serif;margin:0;color:#1b1b1f;background:#fafafa}
main{max-width:1180px;margin:0 auto;padding:24px 16px 80px}
h1{font-size:26px;margin:0 0 4px}h2{margin-top:40px;border-bottom:1px solid #ddd;padding-bottom:6px}
.meta{color:#555}.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(320px,1fr));gap:12px}
.pair{display:grid;grid-template-columns:1fr 1fr;gap:6px;background:#fff;border:1px solid #e3e3e8;border-radius:6px;padding:6px}
figure{margin:0}img{width:100%;height:auto;border:1px solid #eee;display:block}figcaption{font-size:11px;color:#666;overflow-wrap:anywhere}
.entry{background:#fff;border:1px solid #e3e3e8;border-radius:8px;padding:12px 16px;margin:12px 0}
.entry.reverted{opacity:.6}dl{display:grid;grid-template-columns:max-content 1fr;gap:2px 12px;margin:8px 0}dt{color:#666}dd{margin:0}
.missing{color:#a3122a}.verdict{background:#f2f6ff;padding:6px 10px;border-radius:6px}code{font-size:12px}
nav a{margin-right:12px}
table{width:100%;border-collapse:collapse;background:#fff}th,td{text-align:left;vertical-align:top;border-bottom:1px solid #ddd;padding:6px}td code{overflow-wrap:anywhere}details{margin:8px 0}
</style></head><body><main>
<h1>${esc(title ?? "Before and after")}</h1>
<p class="meta">From <code>${esc(from)}</code> (${esc(fromSha.slice(0, 10))}) to <code>${esc(to)}</code> (${esc(toSha.slice(0, 10))}), generated ${esc(new Date().toISOString())}. ${design.length} design, ${feature.length} feature and ${none.length} other change(s).</p>
<nav><a href="#evidence">Verification</a><a href="#together">Together</a><a href="#design">Design</a><a href="#feature">Feature</a><a href="#other">Other</a></nav>
<h2 id="evidence">Verification evidence</h2>${evidence}
<h2 id="together">Together</h2>${sheet || "<p>No capture changed in this range.</p>"}
<h2 id="design">Design changes, step by step</h2>${design.map(entryHtml).join("") || "<p>None.</p>"}
<h2 id="feature">Feature changes, step by step</h2>${feature.map(entryHtml).join("") || "<p>None.</p>"}
<h2 id="other">Other merges (decision: none)</h2>${none.map(entryHtml).join("") || "<p>None.</p>"}
</main></body></html>
`;
	const index = join(dir, "index.html");
	writeFileSync(index, html);
	writeFileSync(join(dir, "entries.json"), `${JSON.stringify(entries, null, 2)}\n`);
	return index;
}

/** The go card and packet for a candidate whose prod run just finished (DEL-34, DEL-71). */
export function writeGoCard({ receipt, cwd }) {
	const dir = join(evidenceDir(), "go");
	mkdirSync(dir, { recursive: true });
	const steps = receipt.steps ?? [];
	const by = (status) => steps.filter((s) => s.status === status);
	const card = `# Go card: ${receipt.candidate}

Candidate commit \`${receipt.commit}\` (tree \`${receipt.tree}\`), against prod \`${receipt.prod}\`.
Prod-candidate receipt: \`${receipt.id}\` — **${receipt.result}**.

| Result | Steps |
| --- | --- |
| Passed | ${by("pass").map((s) => s.id).join(", ") || "none"} |
| Failed | ${by("fail").map((s) => `${s.id} (${(s.reason ?? "").split("\n")[0]})`).join("; ") || "none"} |
| Not run | ${[...by("unrun").map((s) => `${s.id} (${s.reason})`), ...steps.flatMap((s) => notRunParts(s).map((p) => `${s.id}: ${p.what} (${p.reason})`))].join("; ") || "none"} |

${(() => {
	const loaded = steps.find((s) => s.details?.measuredUnderLoad);
	const retried = steps.find((s) => s.details?.loadRetry);
	const parts = [
		retried && loadRetryLine(retried.details.loadRetry),
		loaded && `timing was measured under load (load ${loaded.details.loadBefore} before, ${loaded.details.loadAfter} after; ${loaded.details.stillRunning?.length ? `still running: ${loaded.details.stillRunning.join("; ")}` : "nothing of ours still running"})`,
	].filter(Boolean);
	return parts.length ? `Perf: ${parts.join("; ")}. The verdict is on the budgets alone.\n\n` : "";
})()}${previewReloadLine(steps)}

Changes in the candidate: ${receipt.changes?.length ?? 0} gated merges (see the before-and-after report).

What would deploy where once merged: nothing deploys on merge. Each deploy is its own go (DEL-40).

The one command, run by an agent only after Daniel's go:

\`\`\`
node tools/rv-gate/rv-gate.mjs merge-prod ${receipt.candidate} --go "<Daniel's words, date>"
\`\`\`
`;
	const path = join(dir, `${basename(receipt.candidate)}-${receipt.id}.md`);
	writeFileSync(path, card);
	return path;
}

/** A hotfix is one tested dev patch on the exact prod head recorded before its checks. */
function validateHotfixReceipt({ cwd, receipt, head, prod }) {
	if (receipt.prod !== prod) throw new Error("The hotfix receipt's prod changed; test a new candidate against current prod.");
	const parents = sh("git", ["rev-list", "--parents", "-n", "1", head], { cwd }).split(/\s+/);
	if (parents.length !== 2 || parents[1] !== prod) throw new Error("A hotfix candidate must be exactly one child of current prod.");
	if (!/^(?:[0-9a-f]{40}|[0-9a-f]{64})$/.test(receipt.sourceCommit ?? "") || !isAncestor(cwd, receipt.sourceCommit, "origin/dev"))
		throw new Error("The hotfix source commit must be on current origin/dev.");
	if (sh("git", ["rev-list", "--parents", "-n", "1", receipt.sourceCommit], { cwd }).split(/\s+/).length !== 2)
		throw new Error("The hotfix source must be one non-merge commit on dev.");
	const sourcePatch = patchId(cwd, receipt.sourceCommit);
	if (!sourcePatch || sourcePatch !== patchId(cwd, head)) throw new Error("The hotfix candidate's patch differs from its source commit on dev.");
	if (!Array.isArray(receipt.steps) || !receipt.steps.length) throw new Error("The hotfix receipt has no passing steps.");
	const passed = new Set();
	for (const step of receipt.steps) {
		if (typeof step?.id !== "string" || !step.id || passed.has(step.id)) throw new Error("The hotfix receipt has an invalid or duplicate step.");
		if (step.status !== "pass") throw new Error(`The hotfix receipt's step ${step.id} did not pass.`);
		if (notRunParts(step).length) throw new Error(`The hotfix receipt's step ${step.id} is incomplete: some inputs were not run.`);
		passed.add(step.id);
	}
	const paths = sh("git", ["diff", "--name-only", "--no-renames", "-z", prod, head, "--"], { cwd }).split("\0").filter(Boolean);
	for (const id of hotfixSteps(paths)) if (!passed.has(id)) throw new Error(`The hotfix receipt is missing required step ${id}.`);
	if (!receipt.build || Array.isArray(receipt.build) || typeof receipt.build.snapshot !== "string" || !isAbsolute(receipt.build.snapshot)
		|| !/^[0-9a-f]{64}$/.test(receipt.build.manifestSha256 ?? "")) throw new Error("The hotfix receipt has no valid build snapshot descriptor.");
}

/**
 * Merge a frozen candidate into prod: only with Daniel's recorded go and a passing receipt for the
 * candidate's commit. Since 4 Nov 2026 the full run that promoted pre-prod is that receipt: Daniel
 * checks the visuals and gives his go, with no second test round (DEL-71, S27-883). A prod-candidate
 * receipt, where one exists, is still accepted. A hotfix receipt instead binds one cherry-picked
 * dev patch to the current prod and the checks required by its changed paths.
 */
/** Record a Windows walk (tools/windows-e2e) on the candidate's receipt, once it checks out (ARC-67). */
export function recordWindowsWalk({ candidate, run, cwd }) {
	if (!/^candidate\/\d+$/.test(candidate ?? "") || !run) throw new Error("Usage: rv-gate windows candidate/<n> --run <dir>");
	sh("git", ["fetch", "-q", "origin"], { cwd });
	const head = sh("git", ["rev-parse", `origin/${candidate}`], { cwd });
	const tree = sh("git", ["rev-parse", `${head}^{tree}`], { cwd });
	const receipt = candidateReceipt(listReceipts(), head, candidate);
	if (!receipt) throw new Error(`No receipt for ${candidate} at ${head.slice(0, 10)} to record the Windows walk on.`);
	const walk = readWindowsRun(run, head);
	if (!walk.ok) throw new Error(`The Windows walk in ${run} does not pass for ${head.slice(0, 10)}: ${walk.problems.join("; ")}.`);
	const path = writeReceiptNote(receipt.id, `${WINDOWS_NOTE}-${basename(run).replace(/[^A-Za-z0-9-]/g, "-")}`, { commit: head, tree, candidate, result: "pass", run, ...walk.summary });
	console.log(`Recorded the Windows walk for ${candidate} at ${head.slice(0, 10)}: ${path}.`);
	return 0;
}

export function mergeProd({ candidate, go, cwd, notifyBackmergeFailure }) {
	if (!/^candidate\/\d+$/.test(candidate ?? "")) throw new Error("Usage: rv-gate merge-prod candidate/<n> --go \"<Daniel's words, date>\"");
	if (typeof go !== "string" || go.trim().length < 8) throw new Error("Record Daniel's go with --go \"<his words, date>\" (DEL-04).");
	const unlock = acquireLock("merge-prod", { wait: true });
	try {
		sh("git", ["fetch", "-q", "origin"], { cwd });
		const head = sh("git", ["rev-parse", `origin/${candidate}`], { cwd });
		const tree = sh("git", ["rev-parse", `${head}^{tree}`], { cwd });
		const receipt = candidateReceipt(listReceipts(), head, candidate);
		if (!receipt) throw new Error(`No full, prod-candidate or hotfix receipt for ${candidate} at ${head.slice(0, 10)}; use a passed full or exact hotfix candidate receipt.`);
		if (receipt.result !== "pass") throw new Error(`The receipt ${receipt.id} failed; the candidate cannot merge.`);
		if (receipt.tree !== tree) throw new Error("The candidate's tree differs from its receipt's; cut a new candidate.");
		const release = sh("git", ["show", `${head}:assets/install/release.json`], { cwd, allowFail: true });
		const windows = windowsMergeProblem({ windows: shipsWindows(release.status === 0 ? release.stdout : null), notes: receiptNotes(receipt.id), head });
		if (windows) throw new Error(windows);
		if (receipt.kind === "affected") {
			if (!receiptNotes(receipt.id).some(note => note.kind === "promotion" && note.commit === head && note.tree === tree && note.affectedProof)) throw new Error("Affected receipt has not promoted this candidate to pre-prod.");
			affectedPromotion({cwd, receipt});
		}
		const prod = sh("git", ["rev-parse", "origin/prod"], { cwd });
		if (receipt.kind === "hotfix") validateHotfixReceipt({ cwd, receipt, head, prod });
		if (!isAncestor(cwd, prod, head)) throw new Error("prod is not an ancestor of the candidate; prod receives only what came through dev.");
		const lane = receipt.kind === "hotfix" ? { lane: "hotfix" } : {};
		writeReceiptNote(receipt.id, "go", { go, candidate, commit: head, ...lane });
		// The ancestry check permits only a fast-forward; the exact lease also rejects a prod change
		// made after fetch, even when that newer prod would still be an ancestor of this candidate.
		sh("git", ["push", "-q", `--force-with-lease=refs/heads/prod:${prod}`, "origin", `${head}:refs/heads/prod`], { cwd, env: gateEnv({ RV_GATE_PUSH: "prod" }) });
		if (receipt.kind === "hotfix") {
			try { writeReceiptNote(receipt.id, "prod-merge", { commit: head, tree, ...lane }); }
			catch (error) {
				let reason = `could not record prod-merge: ${error.message}`;
				try { writeReceiptNote(receipt.id, "hotfix-backmerge", { lane: "hotfix", status: "fail", prod: head, pushAttempted: false, reason: `prod already moved to ${head}; ${reason}. Mail the lead before continuing.` }); }
				catch (noteError) { reason += `; could not record the failed back-merge: ${noteError.message}`; }
				throw hotfixFailureError({ receiptId: receipt.id, prod: head, reason, notifyFailure: notifyBackmergeFailure });
			}
			console.log(`prod received ${head.slice(0, 10)} (${candidate}; hotfix lane); go recorded on ${receipt.id}.`);
			const backmerge = backmergeHotfix({ cwd, receiptId: receipt.id, prod: head, notifyFailure: notifyBackmergeFailure });
			console.log(`Hotfix lane: prod is an ancestor of dev at ${backmerge.commit.slice(0, 10)}; fully check this dev tip before the next normal prod promotion.`);
			return 0;
		}
		sh("git", ["fetch", "-q", "origin", "prod"], { cwd });
		const merged = sh("git", ["rev-parse", "origin/prod"], { cwd });
		if (merged !== head) throw new Error("prod changed again after the candidate push; the merge could not be verified.");
		writeReceiptNote(receipt.id, "prod-merge", { commit: merged, tree, ...lane });
		console.log(`prod is now ${merged.slice(0, 10)} (${candidate}${receipt.kind === "hotfix" ? "; hotfix lane" : ""}); go recorded on ${receipt.id}.`);
		return 0;
	} finally { unlock(); }
}
