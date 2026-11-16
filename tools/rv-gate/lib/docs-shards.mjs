// Host reason (ARC-03): admit every original docs journey before publishing merged evidence.
import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

export const DOCS_PHASES = Object.freeze(["native", "main", "desktop", "phone", "no-js"]);
const WIDTHS = [1440, 1024, 768, 390, 320];

/** Derive the complete docs inventory independently of child-declared completion. */
export function docsInventory(root) {
	const guides = readdirSync(join(root, "docs/public")).filter(name => name.endsWith(".md")).map(name => name.slice(0, -3));
	const acceptedPath = join(root, "docs/public/charts/accepted.txt");
	const accepted = existsSync(acceptedPath) ? readFileSync(acceptedPath, "utf8").split("\n").map(line => line.trim()).filter(line => line && !line.startsWith("#")) : null;
	const types = readdirSync(join(root, "docs/public/charts")).filter(name => name.endsWith(".md")).map(name => name.slice(0, -3)).filter(type => !accepted || accepted.includes(type)).sort();
	assert.ok(types.length, "Nonempty authoritative docs chart inventory");
	const statics = ["index", ...guides.filter(name => name !== "index")];
	const pages = [...statics, "charts", "reference", ...types.map(type => `charts/${type}`)];
	const navigation = WIDTHS.flatMap(width => pages.map(name => `${width}:${name}`));
	const observations = {
		native: ["editor-commands", "init", "downloadable-examples", "authoring-snippet", "learn", "report-export", "csv-data", "json-data", "failure-recovery", "style-exports", "mcp", "live-preview", "publication"].map(key => `native/${key}`),
		main: [...WIDTHS.flatMap(width => pages.map(name => `main/navigation/${width}/${name}`)), "main/internal-links", "main/playground", "main/style-contract"],
		desktop: types.map(type => `desktop/${type}`),
		phone: types.map(type => `phone/${type}`),
		"no-js": statics.map(name => `no-js/${name}`),
	};
	return { types, navigation, statics, observations };
}

const exactKeys = (actual, expected, message) => assert.deepEqual([...actual].sort(), [...expected].sort(), message);

/** Verify requested phases against the same independently derived source inventory as a full run. */
export function verifyDocsPhases(reports, { cohort, inventory: expected, engine, selected = DOCS_PHASES }) {
	assert.deepEqual(cohort, DOCS_PHASES, "Complete fixed docs phase cohort");
	assert.ok(Array.isArray(selected) && selected.length && new Set(selected).size === selected.length && selected.every(phase => DOCS_PHASES.includes(phase)), "Valid requested docs phases");
	assert.equal(reports.length, selected.length, "One child for each requested docs phase");
	const byPhase = new Map();
	for (const r of reports) {
		const phase = DOCS_PHASES[r.shard.index];
		assert.ok(selected.includes(phase) && !byPhase.has(phase), "One result per requested docs phase");
		assert.equal(r.shard.count, DOCS_PHASES.length, "Fixed docs shard count");
		assert.deepEqual(r.shard.cohort, DOCS_PHASES, "Full docs shard cohort");
		assert.deepEqual(r.shard.selected, [phase], "Assigned docs phase");
		assert.deepEqual(r.completed, [phase], "Completed docs phase");
		assert.equal(r.admission, "docs", "Actual docs parent admission");
		assert.equal(r.ok, true, "Every docs child must succeed");
		assert.equal(r.engine, engine, "Requested docs engine");
		assert.ok(typeof r.browser_version === "string" && r.browser_version.length, "Actual browser version");
		assert.equal(r.browser_version, reports[0].browser_version, "One browser version");
		assert.deepEqual(r.results, [{ caseKey: phase, ok: true }], "One successful actual phase");
		assert.deepEqual(r.errors, [], "Retain every browser error");
		assert.ok(r.observations.every(row => row.ok === true), "All original journeys succeed");
		exactKeys(r.observations.map(row => row.caseKey), expected.observations[phase], "Every authoritative original journey exactly once");
		assert.deepEqual(r.navigation_cohort, expected.navigation, "Authoritative navigation inventory");
		assert.deepEqual(r.static_cohort, expected.statics, "Authoritative no-JS inventory");
		assert.deepEqual(r.chart_cohort, [1440, 390].flatMap(width => expected.types.map(type => `${width}:${type}`)), "Authoritative chart inventory");
		assert.deepEqual(r.navigation_views, phase === "main" ? expected.navigation : [], "Actual completed navigation views");
		assert.deepEqual(r.static_views, phase === "no-js" ? expected.statics : [], "Actual completed no-JS views");
		const width = phase === "desktop" ? 1440 : phase === "phone" ? 390 : null;
		exactKeys(r.chart_pages.map(row => `${row.width}:${row.type}`), width ? expected.types.map(type => `${width}:${type}`) : [], "Every original chart result");
		assert.ok(r.chart_pages.every(row => row.ok === true), "Every original chart assertion passes");
		exactKeys(r.completed_views, r.chart_pages.map(row => `${row.width}:${row.type}`), "Actual chart completion matches results");
		byPhase.set(phase, r);
	}
	const native = byPhase.get("native"), main = byPhase.get("main"), plain = byPhase.get("no-js");
	for (const value of [...(native ? [native.cli, native.mcp, native.publication, native.failures] : []), ...(main ? [main.browser] : []), ...(plain ? [plain.browser] : [])])
		assert.ok(value && typeof value === "object" && !Array.isArray(value) && Object.keys(value).length, "Original docs evidence payload is present");
	return byPhase;
}

/** Merge the complete fixed cohort into the original canonical evidence. */
export function mergeDocsPhases(reports, options) {
	const byPhase = verifyDocsPhases(reports, options);
	const native = byPhase.get("native"), main = byPhase.get("main"), plain = byPhase.get("no-js");
	return {
		cli: native.cli, mcp: native.mcp, publication: native.publication,
		browser: { ...main.browser, ...plain.browser },
		chart_pages: [...byPhase.get("desktop").chart_pages, ...byPhase.get("phone").chart_pages],
		failures: native.failures,
	};
}
