// Host reason (ARC-03): describe outcomes that runners pass between workers and receipts.
import type { createFirefoxProtocolSink } from "./firefox-protocol.mts";
import type { loadRetryLine } from "./perf.mts";
import type { CaseEvidence } from "./cases.mts";
import type { ChromiumRelaunch, FirefoxPreviewReload } from "./browser-events.mts";

export type GateStatus = "pass" | "fail" | "unrun";

export interface UnitOutcome { id: string; status: GateStatus }
export interface CommandVerdict {
	status: GateStatus;
	reason?: string;
	exitCode?: number;
	id?: string;
	flaky?: boolean;
	environmentRetry?: boolean;
	logPath?: string;
	shards?: (CommandVerdict & { attempts?: (CommandVerdict & { number: number })[] })[];
	caseUnits?: UnitOutcome[];
	cases?: CaseEvidence;
	command?: string;
	firefoxProtocol?: ReturnType<ReturnType<typeof createFirefoxProtocolSink>["end"]>;
	durationMs?: number;
	browserRelaunches?: ChromiumRelaunch[];
	previewReloads?: FirefoxPreviewReload[];
}

export interface StepOutcome {
	status: GateStatus;
	id?: string;
	title?: string;
	cards?: string[];
	reason?: string;
	durationMs?: number;
	runMs?: number;
	queuedMs?: number;
	reuseKey?: string;
	reused?: string;
	flaky?: boolean;
	buildSnapshotArchive?: string;
	exception?: { type: string; until: string; reason: string };
	rotation?: { ordinal: number; cycle: number };
	logs?: string[];
	details?: {
		commands?: CommandVerdict[];
		preparation?: CommandVerdict;
		units?: UnitOutcome[];
		unrunTests?: { name: string; reason: string }[];
		unrunInputs?: { what: string; reason: string }[];
		thresholdChanges?: string[];
		ignored?: unknown[];
		measuredUnderLoad?: boolean;
		loadBefore?: unknown;
		loadAfter?: unknown;
		stillRunning?: string[];
		loadRetry?: Parameters<typeof loadRetryLine>[0];
		browserRelaunches?: ChromiumRelaunch[];
		previewReloads?: FirefoxPreviewReload[];
		[key: string]: unknown;
	};
	attempts?: { status: GateStatus; interrupted?: boolean }[];
	units?: UnitOutcome[];
}

export type ScheduleCommand = <T>(run: () => T, permits?: number) => Promise<Awaited<T>>;
