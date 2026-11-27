// Host reason (ARC-03): restore prod ancestry to dev without changing a checkout or its source tree.
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { PRE_PUSH } from "./hooks.mts";
import { gateEnv, sh } from "./proc.mts";
import { gateHome, writeReceiptNote } from "./store.mts";

interface HotfixFailure { receiptId: string; prod: string; reason: string }
type FailureNotifier = (input: HotfixFailure) => void;
interface BackmergeNote {
	lane: "hotfix";
	status: "pass" | "fail";
	prod: string;
	pushAttempted: boolean;
	dev?: string;
	tree?: string;
	alreadyMerged?: boolean;
	commit?: string;
	reason?: string;
	observed?: ReturnType<typeof remoteHeads>;
}
function mailLead({ receiptId, prod, reason }: HotfixFailure) {
	sh(join(homedir(), "tmp/richview-rewrite/bin/rv-mail"), ["send", "richview-lockdown", `Hotfix receipt ${receiptId}; prod ${prod}. ${reason}`, "--from", "rv-gate"]);
}

/** Notify the lead once and preserve a failed notification in the operator's error. */
export function hotfixFailureError({ receiptId, prod, reason, notifyFailure = mailLead }: HotfixFailure & {notifyFailure?: FailureNotifier}) {
	let message = `prod already moved to ${prod}; ${reason}. Mail the lead before continuing.`;
	try { notifyFailure({ receiptId, prod, reason: message }); }
	catch (error) { message += ` Lead mail failed: ${(error as Error).message}`; }
	return new Error(message);
}

function remoteHeads(cwd: string) {
	const heads = new Map(sh("git", ["ls-remote", "--refs", "origin", "refs/heads/dev", "refs/heads/prod"], { cwd }).split("\n").map(line => {
		const [commit, ref] = line.split(/\s+/);
		return [ref, commit];
	}));
	const dev = heads.get("refs/heads/dev"), prod = heads.get("refs/heads/prod");
	if (![dev, prod].every(commit => /^(?:[0-9a-f]{40}|[0-9a-f]{64})$/.test(commit ?? ""))) throw new Error("Cannot read current remote dev and prod.");
	return { dev: dev!, prod: prod! };
}

function requireHeads(cwd: string, dev: string|undefined, prod: string|undefined) {
	const current = remoteHeads(cwd);
	if (current.prod !== prod) throw new Error("prod changed during the hotfix back-merge.");
	if (current.dev !== dev) throw new Error("dev changed during the hotfix back-merge.");
}

/** The issue dev's newest commits name, so the merge commit names it too (DEL-44). */
function issueOf(cwd: string, dev: string) {
	const messages = sh("git", ["log", "-50", "--format=%B", dev], { cwd });
	return messages.match(/^\s*Refs\s+([A-Z][A-Z0-9]*-\d+)\b/m)?.[1];
}

/** Called with merge-prod's lock held; attach current prod only when dev's exact tree survives. */
export function backmergeHotfix({ cwd, receiptId, prod, notifyFailure }: Omit<HotfixFailure, "reason"> & {cwd: string; notifyFailure?: FailureNotifier}) {
	const note: BackmergeNote = { lane: "hotfix", status: "fail", prod, pushAttempted: false };
	let hooks, failure;
	try {
		sh("git", ["fetch", "-q", "--no-tags", "origin", "+refs/heads/dev:refs/remotes/origin/dev", "+refs/heads/prod:refs/remotes/origin/prod"], { cwd });
		note.dev = sh("git", ["rev-parse", "origin/dev"], { cwd });
		if (sh("git", ["rev-parse", "origin/prod"], { cwd }) !== prod) throw new Error("prod changed after the hotfix promotion.");
		note.tree = sh("git", ["rev-parse", `${note.dev}^{tree}`], { cwd });
		requireHeads(cwd, note.dev, prod);
		if (sh("git", ["merge-base", "--is-ancestor", prod, note.dev], { cwd, allowFail: true }).status === 0) {
			note.alreadyMerged = true;
			note.commit = note.dev;
		} else {
			const merge = sh("git", ["merge-tree", "--write-tree", note.dev, prod], { cwd, allowFail: true });
			if (merge.status !== 0) throw new Error(`The prod back-merge conflicts; no resolution was attempted.\n${merge.stdout || merge.stderr}`);
			if (merge.stdout.trim().split("\n")[0] !== note.tree) throw new Error("The prod back-merge would change the pinned dev tree; no dev push was attempted.");
			const issue = issueOf(cwd, note.dev);
			note.commit = sh("git", ["-c", "commit.gpgSign=false", "commit-tree", note.tree, "-p", note.dev, "-p", prod], { cwd,
				input: `Merge hotfix prod ${prod} back into dev\n\nHotfix receipt: ${receiptId}\n${issue ? `Refs ${issue}\n` : ""}` });
			requireHeads(cwd, note.dev, prod);
			const root = join(gateHome(), "hooks");
			mkdirSync(root, { recursive: true });
			hooks = mkdtempSync(join(root, "hotfix-backmerge-"));
			writeFileSync(join(hooks, "pre-push"), PRE_PUSH, { mode: 0o755 });
			note.pushAttempted = true;
			// prod's no-op refspec detects a changed advertised prod; the caller's merge-prod lock
			// serializes prod writers through the push. The dev lease protects independent dev pushes.
			sh("git", ["-c", `core.hooksPath=${hooks}`, "push", "-q", "--atomic", `--force-with-lease=refs/heads/dev:${note.dev}`,
				`--force-with-lease=refs/heads/prod:${prod}`, "origin", `${note.commit}:refs/heads/dev`, `${prod}:refs/heads/prod`], { cwd, env: gateEnv({ RV_GATE_PUSH: "hotfix-backmerge" }) });
		}
		requireHeads(cwd, note.commit, prod);
		note.status = "pass";
	} catch (error) {
		note.reason = `prod already moved to ${prod}; hotfix back-merge failed: ${(error as Error).message}. Mail the lead before continuing.`;
		try { note.observed = remoteHeads(cwd); } catch { /* Preserve the original failure when the remote cannot be read. */ }
		failure = error;
	} finally {
		if (hooks) rmSync(hooks, { recursive: true, force: true });
	}
	try { writeReceiptNote(receiptId, "hotfix-backmerge", note); }
	catch (error) { throw hotfixFailureError({ receiptId, prod, reason: `could not record the hotfix back-merge result: ${(error as Error).message}`, notifyFailure }); }
	if (failure) throw hotfixFailureError({ receiptId, prod, reason: `hotfix back-merge failed: ${(failure as Error).message}`, notifyFailure });
	return note;
}
