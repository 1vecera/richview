// Host reason (ARC-03): admit complete verification commands under the full runner's worker budget.
import { runLogged } from "./proc.mts";
import { runRustTests } from "./rust-tests.mts";
import { runShardedCommand } from "./shards.mts";
import { browserEventJournal } from "./browser-events.mts";
import { releaseBuildModeArgs } from "./build-mode.mts";
import type { GateCommand } from "./steps.mts";
import type { CommandVerdict } from "./results.mts";
import type { ShardSchedule } from "./shards.mts";

interface CommandOptions {
	cwd: string;
	env?: NodeJS.ProcessEnv;
	logPath: string;
	signal?: AbortSignal;
	parallel?: number;
	sourceHold?: boolean;
	mode?: string;
	scheduleCommand?: ShardSchedule;
}

/** Reserve every child worker before enabling a command's bounded internal pool. */
export async function runScheduledCommand(command: GateCommand, args: readonly string[], { cwd, env, logPath, signal, parallel = 1, sourceHold = false, mode, scheduleCommand = run => run() }: CommandOptions): Promise<CommandVerdict> {
	if (!Number.isSafeInteger(parallel) || parallel < 1) throw new Error("Command parallelism must be a positive integer");
	const options = { cwd, env, logPath, signal };
	const argv = [...args, ...(command.installReleaseMode ? releaseBuildModeArgs(cwd) : [])];
	// Serial/candidate and the legacy release executor retain their existing execution exactly.
	if (parallel === 1 || mode !== "full") return scheduleCommand(() => runLogged(command.cmd, argv, options));
	let permits = 1;
	const admitted: NodeJS.ProcessEnv = { ...env, RICHVIEW_VERIFY_WORKERS: "1" };
	if (command.workers) {
		const { env: variable, arg, count } = command.workers;
		if (!Number.isSafeInteger(count) || count < 1 || Boolean(variable) === Boolean(arg) || variable && variable !== "RICHVIEW_VERIFY_WORKERS" || arg && !/^--[a-z][a-z-]*$/.test(arg))
			throw new Error("Command workers need a positive count and one supported env or argument");
		if (command.shard) throw new Error("A command cannot combine internal workers and process shards");
		permits = Math.min(count, parallel);
		if (variable) admitted[variable] = String(permits);
		else {
			if (argv.some(value => value === arg || value.startsWith(`${arg}=`))) throw new Error(`Worker argument ${arg} already exists`);
			argv.push(arg!, String(permits));
		}
	}
	if (command.shard) return runShardedCommand(command.cmd, argv, { ...options, env: admitted, parallel, scheduleCommand, shard: command.shard });
	const rust = sourceHold && command.cmd === "cargo" && argv[0] === "test" && argv.includes("--workspace") && argv.includes("--no-fail-fast");
	if (rust && command.workers) throw new Error("Rust tests use their own reserved worker assignment");
	if (rust) permits = Math.min(2, parallel);
	return scheduleCommand(async () => {
		if (!rust) return runLogged(command.cmd, argv, { ...options, env: admitted });
		// Every held Rust child shares the outer command's journal, including doctests and fallbacks.
		const journal = browserEventJournal(logPath, admitted);
		return journal.collect(await runRustTests({ ...options, env: journal.env, args: argv, workers: permits }));
	}, permits);
}
