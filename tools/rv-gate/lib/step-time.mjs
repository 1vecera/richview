// Host reason (ARC-03): measure admitted gate execution and interpret retained command timing evidence.
const milliseconds = value => typeof value === "number" && Number.isFinite(value) && value >= 0;
const unknown = () => ({ runMs: null, queuedMs: null, source: "unknown" });

function commandTime(text) {
	const footers = [...text.matchAll(/^rv-gate: exit -?\d+ after (\d+(?:\.\d+)?) ms\r?$/gm)];
	if (!footers.length) return null;
	const last = footers.at(-1), duration = Number(last[1]);
	return milliseconds(duration) ? { duration, end: last.index + last[0].length } : null;
}

function logTime(text) {
	const count = /^\$ [^\r\n]+ \((\d+) shards, command budget \d+\)\r?$/m.exec(text.split("\n", 1)[0]);
	if (!count) {
		const timing = commandTime(text);
		return timing && !text.slice(timing.end).trim() ? timing.duration : null;
	}
	// A shard dispatcher forwards child logs; it has no enclosing command lifetime.
	const total = Number(count[1]), sections = [...text.matchAll(/^--- shard (\d+)\/(\d+) ---\r?$/gm)], seen = new Set();
	if (!Number.isSafeInteger(total) || total < 2 || sections.length !== total) return null;
	let longest = 0;
	for (let index = 0; index < sections.length; index++) {
		const part = sections[index], ordinal = Number(part[1]);
		if (Number(part[2]) !== total || ordinal >= total || seen.has(ordinal)) return null;
		seen.add(ordinal);
		const textPart = text.slice(part.index + part[0].length, sections[index + 1]?.index);
		const timing = commandTime(textPart);
		if (!timing) return null;
		longest = Math.max(longest, timing.duration);
	}
	return longest;
}

/** Prefer measured execution, serial elapsed time, then a conservative command-log lower bound. */
export function receiptStepTime(step, receipt, { readLog } = {}) {
	if (milliseconds(step.runMs)) {
		const queuedMs = milliseconds(step.queuedMs) ? step.queuedMs
			: milliseconds(step.durationMs) && step.durationMs >= step.runMs ? step.durationMs - step.runMs : null;
		return { runMs: step.runMs, queuedMs, source: "telemetry" };
	}
	if (["prod", "full"].includes(receipt.kind) && receipt.parallel === 1 && milliseconds(step.durationMs))
		return { runMs: step.durationMs, queuedMs: 0, source: "serial-duration" };
	if (typeof readLog !== "function" || !Array.isArray(step.logs) || !step.logs.length) return unknown();
	const durations = [];
	for (const path of new Set(step.logs)) {
		let text;
		try { text = readLog(path); } catch { return unknown(); }
		if (typeof text !== "string" && !ArrayBuffer.isView(text)) return unknown();
		const duration = logTime(typeof text === "string" ? text : new TextDecoder().decode(text));
		if (duration === null) return unknown();
		durations.push(duration);
	}
	// Logs lack absolute start times: overlap and total queue time cannot be reconstructed.
	return { runMs: Math.max(...durations), queuedMs: null, source: "log-footer-lower-bound" };
}

/** Union admitted execution intervals; time outside their union is reported as queued time. */
export function executionMeter({ now = () => performance.now() } = {}) {
	const intervals = [];
	let active = 0;
	async function measure(run) {
		const start = now();
		active++;
		try { return await run(); }
		finally { intervals.push([start, now()]); active--; }
	}
	return {
		measure,
		schedule: scheduleCommand => (run, permits) => scheduleCommand(() => measure(run), permits),
		finish(durationMs) {
			if (active) throw new Error("Cannot finish timing while execution is active");
			if (!milliseconds(durationMs)) throw new Error("Step duration must be finite and nonnegative");
			let runMs = 0, right = -Infinity;
			for (const [start, end] of [...intervals].sort(([a], [b]) => a - b)) {
				if (!milliseconds(start) || !milliseconds(end) || end < start) throw new Error("Execution clock must be finite and monotonic");
				runMs += Math.max(0, end - Math.max(start, right));
				right = Math.max(right, end);
			}
			return { runMs, queuedMs: Math.max(0, durationMs - runMs) };
		},
	};
}
