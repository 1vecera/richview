// Host reason (ARC-03): execute affected browser cases and admit only their validated evidence.
import {createHash} from "node:crypto";
import {mkdirSync, readFileSync, rmSync} from "node:fs";
import {join} from "node:path";
import {runShardedCommand} from "./shards.mjs";
import {verifyBrowserCases} from "./browser-cases.mjs";
import {verifyFigureCases} from "./figure-cases.mjs";

const same = (a,b) => JSON.stringify(a) === JSON.stringify(b);
function outputArgs(args, out) {
	const result = [...args], at = result.indexOf("--out");
	if (at < 0) result.push("--out", out);
	else if (at + 1 >= result.length) throw new Error("Case command has no output argument");
	else result[at + 1] = out;
	return result;
}

/** Run exactly the selected keys and return pass only after their independent evidence validates. */
export async function executeCaseCommand({command, args, selection, cwd, runDir, env, logPath, signal, parallel, schedule, runCommand, runShards = runShardedCommand}) {
	if (!Array.isArray(selection) || !selection.length || typeof runCommand !== "function" || typeof schedule !== "function") throw new Error("Case execution needs selected units and a scheduler");
	const kind = selection[0].caseKind, keys = selection.map(item => item.caseKey);
	if (!["runtime", "docs", "browser", "figure-expand"].includes(kind) || selection.some(item => item.caseKind !== kind || typeof item.caseKey !== "string" || !item.caseKey) || new Set(keys).size !== keys.length) throw new Error("Invalid affected case selection");
	const id = selection[0].commandId;
	if (typeof id !== "string" || !id || selection.some(item => item.commandId !== id)) throw new Error("Affected cases do not share one command");
	const out = join(runDir, "affected-cases", createHash("sha256").update(id).digest("hex").slice(0,16));
	rmSync(out, {recursive:true, force:true}); mkdirSync(out, {recursive:true});
	const commandArgs = outputArgs(args, out);
	let result, evidence;
	try {
		if (["runtime", "docs"].includes(kind)) {
			let tail = Promise.resolve();
			const admit = parallel === 1 ? work => { const next = tail.then(() => schedule(work)); tail = next.catch(() => {}); return next; } : schedule;
			result = await runShards(command.cmd, commandArgs, {cwd, env, logPath, signal, parallel, scheduleCommand:admit,
				shard:{kind, count:kind === "docs" ? 5 : 2, out, cases:keys}});
			if (result.status === "pass") {
				evidence = result.cases;
				if (!evidence || !same(evidence.selected, keys) || !same(evidence.completed, keys)) throw new Error("Shard evidence does not match selected cases");
			}
		} else {
			const caseEnv = {...env, RICHVIEW_VERIFY_PARENT:`affected-${kind}`, RICHVIEW_VERIFY_CASES:JSON.stringify(keys)};
			result = await runCommand(commandArgs, caseEnv);
			if (result.status === "pass") {
				const filename = kind === "browser" ? "browser.partial.json" : "figure-expand.partial.json";
				const report = JSON.parse(readFileSync(join(out, filename), "utf8"));
				evidence = kind === "browser" ? verifyBrowserCases(report, keys) : verifyFigureCases(report, keys);
			}
		}
		if (!result || !["pass", "fail", "unrun"].includes(result.status)) throw new Error("Case command has an invalid outcome");
	} catch (error) { result = {...result, status:"fail", reason:`Affected case evidence: ${error.message}`}; }
	return {...result, ...(evidence && {cases:evidence}), caseUnits:selection.map(item => ({id:item.id,status:result.status}))};
}
