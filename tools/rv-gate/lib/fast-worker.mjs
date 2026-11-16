// Host reason (ARC-03): execute one complete local fast attempt in a fresh Node process.
import {readFileSync} from "node:fs";
import {fastGate} from "./fast.mjs";
try { process.exitCode = await fastGate({...JSON.parse(readFileSync(process.argv[2], "utf8")), recordPass: false}); }
catch (error) { console.error(`rv-gate fast: ${error.stack ?? error}`); process.exitCode = 1; }
