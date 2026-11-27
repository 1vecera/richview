// Host reason (ARC-03): retain direct-function and child-command browser relaunches and preview reloads in full receipts.
import { isAbsolute } from "node:path";
import { rmSync } from "node:fs";
import { collectBrowserEvents } from "./browser-events.mts";
import type { ChromiumRelaunch, FirefoxPreviewReload } from "./browser-events.mts";
import type { GateStatus } from "./results.mts";

interface BrowserEvidenceStep {
	id: string;
	details?: { browserRelaunches?: readonly ChromiumRelaunch[]; previewReloads?: readonly FirefoxPreviewReload[] };
}

/** Collect one parent journal after every full step drains, then restore the caller's environment. */
export async function withBrowserEvents<T extends readonly BrowserEvidenceStep[]>(file: string, run: () => T | Promise<T>) {
	if (!isAbsolute(file)) throw new Error("Full browser relaunch journal must be absolute");
	rmSync(file, { force: true });
	const previous = process.env.RICHVIEW_BROWSER_EVENT_LOG;
	process.env.RICHVIEW_BROWSER_EVENT_LOG = file;
	try {
		const results = await run();
		const parent = collectBrowserEvents(file, { status: "pass" as const });
		return {
			results,
			browserRelaunches: [
				...results.flatMap(step => (step.details?.browserRelaunches ?? []).map(event => ({ ...event, step: step.id }))),
				...parent.browserRelaunches,
			],
			previewReloads: [
				...results.flatMap(step => (step.details?.previewReloads ?? []).map(event => ({ ...event, step: step.id }))),
				...(parent.previewReloads ?? []),
			],
			browserRelaunchTelemetry: { status: parent.status as GateStatus, journal: file, ...(parent.reason && { reason: parent.reason }) },
		};
	} finally {
		if (previous === undefined) delete process.env.RICHVIEW_BROWSER_EVENT_LOG;
		else process.env.RICHVIEW_BROWSER_EVENT_LOG = previous;
	}
}
