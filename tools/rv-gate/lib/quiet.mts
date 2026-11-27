// Host reason (ARC-03): keep new browser work out of the benchmark's existing quiet window.
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

/** The same shared-target hold the Cargo shim observes, including dedicated release runs. */
export const QUIET_FILE = join(resolve(process.env.RV_SHARED_TARGET || join(homedir(), "tmp/richview-rewrite/target")), ".rv-perf-quiet");

function benchmarkOwner(file: string) {
	let text;
	try { text = readFileSync(file, "utf8").trim(); }
	catch (error) { if ((error as NodeJS.ErrnoException).code === "ENOENT") return 0; throw error; }
	const pid = Number(text);
	if (!/^[1-9]\d*$/.test(text) || !Number.isSafeInteger(pid) || pid > 0x7fffffff) return 0;
	try { process.kill(pid, 0); return pid; }
	catch (error) {
		const { code } = error as NodeJS.ErrnoException;
		if (code === "ESRCH") return 0;
		if (code === "EPERM") return pid;
		throw error;
	}
}

/** Wait for another benchmark owner; the owner itself may launch its measured browser. */
export async function waitForBenchmark({ file = QUIET_FILE, signal }: { file?: string; signal?: AbortSignal } = {}) {
	let announced: number | undefined;
	for (;;) {
		signal?.throwIfAborted();
		const pid = benchmarkOwner(file);
		if (!pid || pid === process.pid) return;
		if (pid !== announced) {
			console.error(`waiting for performance measurement (pid ${pid}) before starting a browser`);
			announced = pid;
		}
		await sleep(250, undefined, { signal });
	}
}
