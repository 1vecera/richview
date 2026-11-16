// Host reason (ARC-03): retain both local fast attempts without making edit-loop output release evidence.
import {writeFileSync} from "node:fs";
import {join} from "node:path";
import {fileURLToPath} from "node:url";
import {randomBytes} from "node:crypto";
import {fastGate} from "./fast.mjs";
import {gateEnv, runLogged} from "./proc.mjs";
import {ensureStore, runDir} from "./store.mjs";
import {readPhase} from "./phase.mjs";
import {withRerun} from "./rerun.mjs";
import {recordFastPass} from "./fastpass.mjs";

/** Failed fast work reruns as a complete fresh invocation, including its static checks. */
export async function fastInPhase(options) {
	if (options.plan) return fastGate(options);
	const phasePolicy = readPhase(); ensureStore();
	const dir = runDir(`fast-local-${Date.now()}-${randomBytes(6).toString("hex")}`);
	const input = join(dir, "options.json"); writeFileSync(input, JSON.stringify(options));
	const controller = new AbortController(), stop = () => controller.abort();
	process.once("SIGINT", stop); process.once("SIGTERM", stop);
	try {
		const result = await withRerun(phasePolicy, async number => {
			const logPath = join(dir, `attempt-${number}.log`);
			const outcome = await runLogged(process.execPath, [fileURLToPath(new URL("./fast-worker.mjs", import.meta.url)), input], {
				cwd: options.cwd, env: gateEnv(), logPath, signal: controller.signal, onOutput: chunk => process.stdout.write(chunk),
			});
			return {...outcome, logs: [logPath]};
		}, {signal: controller.signal});
		writeFileSync(join(dir, "result.json"), JSON.stringify({kind: "local-fast", phase: phasePolicy.phase, phasePolicy, ...result}, null, 2) + "\n");
		if (result.status === "pass") {
			const recorded = recordFastPass(options.cwd);
			console.log(recorded.refused ? `  not recorded for the pre-push hook: ${recorded.refused}` : `  recorded for the pre-push hook: ${recorded.commits} commit(s) up to ${recorded.head.slice(0, 10)}`);
		}
		if (result.flaky) console.log("fast: FLAKY (passed on its single fresh prelaunch rerun)");
		console.log(`Local fast attempts: ${join(dir, "result.json")} (ordinary run-log retention; not release evidence)`);
		return result.status === "pass" ? 0 : 1;
	} finally { process.off("SIGINT", stop); process.off("SIGTERM", stop); }
}
