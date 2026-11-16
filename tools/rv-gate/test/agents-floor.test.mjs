// S27-894: the disk floor AGENTS.md tells agents is the one the gate enforces. On 5 Nov 2026 an agent
// obeyed a stale AGENTS.md (40 GiB) while the gate and the Cargo shim enforced 15 GiB, and stopped
// building for 2 hours with 22 GiB free.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { DISK_FLOOR_GIB } from "../lib/proc.mjs";

test("AGENTS.md states the gate's disk floor", () => {
	const agents = readFileSync(new URL("../../../AGENTS.md", import.meta.url), "utf8");
	const stated = [...agents.matchAll(/Disk floor: (\d+) GiB/g)].map((match) => Number(match[1]));
	assert.deepEqual(stated, [DISK_FLOOR_GIB], "AGENTS.md must state the floor once, as tools/rv-gate/lib/proc.mjs's DISK_FLOOR_GIB");
});
