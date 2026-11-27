// Host reason (ARC-03): retain every bounded browser relaunch and preview reload even when a command fails or buffers stderr.
import { existsSync, readFileSync, rmSync } from "node:fs";
import { isAbsolute, resolve } from "node:path";
import { PREVIEW_RELOAD } from "./preview-reload.mts";

type BrowserEvent = Record<string, unknown>;
type PreviewReload = BrowserEvent & { recovery: number; maxRecoveries: number; case: string; screenshot: string };
export type FirefoxPreviewReload = PreviewReload & { journal: string };
export type ChromiumRelaunch = BrowserEvent & { surface: "pdf" | "playwright"; journal: string };
type JournalOutcome = { status: string; reason?: string; exitCode?: number | null };
type JournalResult<T extends JournalOutcome> = Omit<T, "status"> & {
	status: T["status"] | "fail";
	reason?: string;
	exitCode?: number | null;
	browserRelaunches: ChromiumRelaunch[];
	previewReloads?: FirefoxPreviewReload[];
};

/** Assign one fresh, absolute journal to a command and collect its relaunch evidence on completion. */
export function browserEventJournal(logPath: string, env?: NodeJS.ProcessEnv) {
	const file = resolve(`${logPath}.browser-events.jsonl`);
	rmSync(file, { force: true });
	const journalEnv: NodeJS.ProcessEnv = { ...env, RICHVIEW_BROWSER_EVENT_LOG: file };
	return {
		file,
		env: journalEnv,
		collect: <T extends JournalOutcome>(outcome: T) => collectBrowserEvents(file, outcome),
	};
}

/** A Firefox preview reload as preview-reload.mjs writes it: within its limit, naming its case and a saved screenshot. */
// Historical journals used three per run and one per case; preserve those stricter bounds.
const legacyPreviewLimit = 3;
const previewReload = (event: BrowserEvent): event is PreviewReload =>
	event.version === 1 && event.surface === "playwright" && event.engine === "firefox" && event.issue === PREVIEW_RELOAD.issue
	&& (event.maxRecoveries === legacyPreviewLimit || event.maxRecoveries === PREVIEW_RELOAD.maxRecoveries)
	&& typeof event.recovery === "number" && Number.isSafeInteger(event.recovery) && event.recovery >= 1 && event.recovery <= event.maxRecoveries
	&& typeof event.case === "string" && event.case.trim() !== "" && typeof event.screenshot === "string" && isAbsolute(event.screenshot) && existsSync(event.screenshot);

/** Read a command or parent journal without clearing it; invalid evidence fails the supplied outcome. */
export function collectBrowserEvents<T extends JournalOutcome>(file: string, outcome: T): JournalResult<T> {
	const browserRelaunches: ChromiumRelaunch[] = [], previewReloads: FirefoxPreviewReload[] = [];
	try {
		if (!isAbsolute(file)) throw new Error("Browser relaunch journal must be absolute");
		let text;
		try { text = readFileSync(file, "utf8"); } catch (error) { if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error; text = ""; }
		for (const line of text.split("\n").filter(value => value.trim())) {
			const event: BrowserEvent | null = JSON.parse(line);
			if (event?.event === "firefox-preview-reload") {
				if (!previewReload(event) || event.recovery !== previewReloads.length + 1) throw new Error("Invalid bounded Firefox preview reload event");
				if (previewReloads.some(reload => reload.maxRecoveries !== event.maxRecoveries)) throw new Error("The preview reload limit changed within a journal");
				const perCase = event.maxRecoveries === legacyPreviewLimit ? 1 : PREVIEW_RELOAD.maxPerCase;
				if (previewReloads.filter(reload => reload.case === event.case).length >= perCase) throw new Error(`The preview exceeded its per-case reload limit: ${event.case}`);
				previewReloads.push({ ...event, journal: file });
				continue;
			}
			if (event?.version !== 1 || event.event !== "browser-startup-relaunch" || event.surface !== "pdf" && event.surface !== "playwright" || event.engine !== "chromium" || event.attempt !== 2 || event.maxAttempts !== 2 || typeof event.timeoutMs !== "number" || !Number.isSafeInteger(event.timeoutMs) || event.timeoutMs < 1 || event.timeoutMs > 15000 || event.pid !== undefined && (typeof event.pid !== "number" || !Number.isSafeInteger(event.pid) || event.pid < 1))
				throw new Error("Invalid bounded Chromium relaunch event");
			browserRelaunches.push({ ...event, surface: event.surface, journal: file });
		}
		if (text && !text.endsWith("\n")) throw new Error("Incomplete browser relaunch journal line");
		return { ...outcome, browserRelaunches, ...(previewReloads.length && { previewReloads }) };
	} catch (error) {
		return { ...outcome, status: "fail" as const, exitCode: outcome.exitCode || 1, reason: [outcome.reason, `Could not read browser relaunch journal ${file}: ${(error as Error).message}`].filter(Boolean).join("; "), browserRelaunches, ...(previewReloads.length && { previewReloads }) };
	}
}

/** Read validated relaunches for a receipt; invalid evidence throws with any preceding events attached. */
export function readBrowserRelaunches(file: string) {
	const result = collectBrowserEvents<JournalOutcome>(file, { status: "pass", exitCode: 0 });
	if (result.status === "fail") {
		throw Object.assign(new Error(result.reason), { browserRelaunches: result.browserRelaunches });
	}
	return result.browserRelaunches;
}
