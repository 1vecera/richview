// Host reason (ARC-03): retain every bounded browser relaunch and preview reload even when a command fails or buffers stderr.
import { existsSync, readFileSync, rmSync } from "node:fs";
import { isAbsolute, resolve } from "node:path";
import { PREVIEW_RELOAD } from "./preview-reload.mjs";

/** Assign one fresh, absolute journal to a command and collect its relaunch evidence on completion. */
export function browserEventJournal(logPath, env) {
	const file = resolve(`${logPath}.browser-events.jsonl`);
	rmSync(file, { force: true });
	return {
		file,
		env: { ...env, RICHVIEW_BROWSER_EVENT_LOG: file },
		collect: outcome => collectBrowserEvents(file, outcome),
	};
}

/** A Firefox preview reload as preview-reload.mjs writes it: within its limit, naming its case and a saved screenshot. */
// Historical journals used three per run and one per case; preserve those stricter bounds.
const legacyPreviewLimit = 3;
const previewReload = event => event.version === 1 && event.surface === "playwright" && event.engine === "firefox" && event.issue === PREVIEW_RELOAD.issue
	&& [legacyPreviewLimit, PREVIEW_RELOAD.maxRecoveries].includes(event.maxRecoveries) && Number.isSafeInteger(event.recovery) && event.recovery >= 1 && event.recovery <= event.maxRecoveries
	&& typeof event.case === "string" && event.case.trim() !== "" && typeof event.screenshot === "string" && isAbsolute(event.screenshot) && existsSync(event.screenshot);

/** Read a command or parent journal without clearing it; invalid evidence fails the supplied outcome. */
export function collectBrowserEvents(file, outcome) {
	const browserRelaunches = [], previewReloads = [];
	try {
		if (!isAbsolute(file)) throw new Error("Browser relaunch journal must be absolute");
		let text;
		try { text = readFileSync(file, "utf8"); } catch (error) { if (error.code !== "ENOENT") throw error; text = ""; }
		for (const line of text.split("\n").filter(value => value.trim())) {
			const event = JSON.parse(line);
			if (event?.event === "firefox-preview-reload") {
				if (!previewReload(event) || event.recovery !== previewReloads.length + 1) throw new Error("Invalid bounded Firefox preview reload event");
				if (previewReloads.some(reload => reload.maxRecoveries !== event.maxRecoveries)) throw new Error("The preview reload limit changed within a journal");
				const perCase = event.maxRecoveries === legacyPreviewLimit ? 1 : PREVIEW_RELOAD.maxPerCase;
				if (previewReloads.filter(reload => reload.case === event.case).length >= perCase) throw new Error(`The preview exceeded its per-case reload limit: ${event.case}`);
				previewReloads.push({ ...event, journal: file });
				continue;
			}
			if (event?.version !== 1 || event.event !== "browser-startup-relaunch" || !["pdf", "playwright"].includes(event.surface) || event.engine !== "chromium" || event.attempt !== 2 || event.maxAttempts !== 2 || !Number.isSafeInteger(event.timeoutMs) || event.timeoutMs < 1 || event.timeoutMs > 15000 || event.pid !== undefined && (!Number.isSafeInteger(event.pid) || event.pid < 1))
				throw new Error("Invalid bounded Chromium relaunch event");
			browserRelaunches.push({ ...event, journal: file });
		}
		if (text && !text.endsWith("\n")) throw new Error("Incomplete browser relaunch journal line");
		return { ...outcome, browserRelaunches, ...(previewReloads.length && { previewReloads }) };
	} catch (error) {
		return { ...outcome, status: "fail", exitCode: outcome.exitCode || 1, reason: [outcome.reason, `Could not read browser relaunch journal ${file}: ${error.message}`].filter(Boolean).join("; "), browserRelaunches, ...(previewReloads.length && { previewReloads }) };
	}
}

/** Read validated relaunches for a receipt; invalid evidence throws with any preceding events attached. */
export function readBrowserRelaunches(file) {
	const result = collectBrowserEvents(file, { status: "pass", exitCode: 0 });
	if (result.status === "fail") {
		const error = new Error(result.reason);
		error.browserRelaunches = result.browserRelaunches;
		throw error;
	}
	return result.browserRelaunches;
}
