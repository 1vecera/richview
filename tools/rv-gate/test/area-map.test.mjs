// Host reason (ARC-03): keep the affected map aligned with the executable gate registry.
import test from "node:test";
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import {stepsForTier} from "../lib/steps.mjs";
import {validateAreas, selectAffected} from "../lib/selection.mjs";

const areas = JSON.parse(readFileSync(new URL("../areas.json", import.meta.url), "utf8"));
const steps = stepsForTier("full", "full");
test("the checked-in map names existing steps and commands", () => {
	assert.doesNotThrow(() => validateAreas(areas, steps));
});
test("typos and malformed paths fail closed", () => {
	for (const mutate of [map => map.areas[0].steps.push("missing-step"), map => map.areas[0].paths.push("../outside"), map => map.areas[1].commands.chromium.push("scripts/absent.mjs")]) {
		const broken = structuredClone(areas); mutate(broken);
		assert.throws(() => validateAreas(broken, steps));
	}
});

test("a home-page change selects home and shared site checks without chart suites", () => {
	const changed = ["assets/landing/bootstrap.js", "assets/landing/homepage.css", "assets/landing/index.html", "crates/richview-build/src/chrome.rs", "crates/richview-site/src/browser/voices.rs", "scripts/verify-browser.mjs"];
	const plan = selectAffected(steps, changed, {areas, read: () => null});
	const chromium = plan.filter(item => item.step === "chromium" && item.status === "run").map(item => item.command);
	assert.ok(chromium.some(command => command.includes("verify-browser.mjs")));
	assert.ok(chromium.some(command => command.includes("verify-docs.mjs")));
	assert.ok(!chromium.some(command => command.includes("verify-chart-interaction.mjs")));
	for (const id of ["chart-type-keyboard", "chart-type-pdf", "theme-switch"]) assert.equal(plan.find(item => item.id === id).status, "skipped");
	assert.ok(plan.filter(item => item.step === "data-integrity").every(item => item.status === "skipped"));
	assert.ok(plan.every(item => !item.reason.includes("Unknown product path")));
});

test("the starter template change and its CLI/Publish inputs reach their journeys", () => {
	const changes = ["crates/richview-app/tests/report_folders.rs", "examples/hello-world/report.rmd"];
	for (const paths of [changes, ["examples/hello-world/data/visits.csv"], ["crates/richview-app/src/ops.rs"], ["crates/richview-publish-server/src/lib.rs"], ["richview-chart/src/lib.rs"], ["Cargo.lock"]]) {
		const plan = selectAffected(steps, paths, {areas, read: () => null});
		assert.equal(plan.find(item => item.id === "journeys-full").status,"run",paths.join(", "));
		if (!paths[0].includes("publish-server")) assert.equal(plan.find(item => item.id === "journeys").status,"run",paths.join(", "));
	}
});
