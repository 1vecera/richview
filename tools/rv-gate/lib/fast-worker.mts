// Host reason (ARC-03): execute one complete local fast attempt in a fresh Node process.
import {readFileSync} from "node:fs";
import {fastGate} from "./fast.mts";
import type {FastOptions} from "./fast.mts";
try {
	const options: FastOptions = JSON.parse(readFileSync(process.argv[2], "utf8"));
	process.exitCode = await fastGate({...options, recordPass: false});
}
catch (error) { console.error(`rv-gate fast: ${(error as Error).stack ?? error}`); process.exitCode = 1; }
