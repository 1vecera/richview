// Host reason (ARC-03): select and validate the checks for one production hotfix.
// Unknown paths run both browser groups: an unclassified change must not silently skip them.
export function hotfixSteps(paths) {
	let runtime = false, site = false;
	const scripts = new Set();
	const chartChrome = new Set([
		"crates/richview-core/src/figure.rs",
		"crates/richview-core/src/style/components.rs",
		"crates/richview-runtime/src/web/host.rs",
		"richview-chart/src/interaction.rs",
	]);
	for (const path of paths) {
		if (chartChrome.has(path)) {
			scripts.add("scripts/verify-figure-expand.mjs");
			scripts.add("scripts/verify-chart-keyboard.mjs");
		}
		if (/^scripts\/verify-[^/]+\.mjs$/.test(path)) scripts.add(path);
		if (path.startsWith("tools/")) continue;
		if (path.startsWith("assets/")) { site = true; continue; }
		if (path.startsWith("richview-chart/") || path.startsWith("crates/richview-core/")) { runtime = true; site = true; continue; }
		if (path.startsWith("crates/richview-site/")) { runtime = true; site = true; continue; }
		runtime = true; site = true;
	}
	const named = new Map([
		["scripts/verify-report-runtime.mjs", "report-runtime"],
		["scripts/verify-chart-review.mjs", "report-visual"],
		["scripts/verify-site.mjs", "verify-site"],
		["scripts/verify-browser.mjs", "site-browser"],
	]);
	const steps = new Set(["fast", "build", ...(runtime ? ["report-runtime", "report-visual"] : []), ...(site ? ["verify-site", "site-browser"] : [])]);
	for (const script of [...scripts].sort()) steps.add(named.get(script) ?? `script:${script}`);
	return [...steps];
}
