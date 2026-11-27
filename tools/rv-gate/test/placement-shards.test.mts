// Host reason (ARC-03): keep the chromium step's per-style placement commands equal to the whole run.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { FIGURE_THEMES } from "../lib/figure-cases.mts";
import { STEPS } from "../lib/steps.mts";

const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const script = "scripts/verify-readout-placement.ts";

test("the chromium step runs the readout placement script once for every style, and no style is left out of the script's own list", () => {
	const chromium = STEPS.find(step => step.id === "chromium");
	assert.ok(chromium?.run);
	const commands = chromium.run.filter(command => command.entry === script);
	assert.deepEqual(commands.map(command => command.args), FIGURE_THEMES.map(style => [script, `--styles=${style}`]));
	assert.equal(new Set(commands.map(command => command.args.join(" "))).size, commands.length, "each command has its own label");
	const text = readFileSync(join(root, script), "utf8");
	const listed = /only\('RV_PLACEMENT_STYLES', \[([^\]]*)\]\)/.exec(text)?.[1].split(",").map(item => item.trim().replace(/'/g, ""));
	assert.ok(listed?.length, "the script names its default styles");
	assert.deepEqual([...listed].sort(), [...FIGURE_THEMES].sort(), "a style added to one list must be added to the other, or the gate would skip it");
});
