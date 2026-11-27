// Host reason (ARC-03): require every admitted no-JS mode, width and recovery observation.
const WIDTHS = [1280, 390], MODES = ["live", "no-js", "no-webgl"], SALES = "sales-report";
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const requireThat = (condition: boolean, message: string|undefined) => { if (!condition) throw new Error(message); };
type ReportCase = { name: string; width: number; figures: number };
type ReportView = { name: string; width: number; mode: string };
type Recovery = { missing: string; present: string };
type ReportShard = {
	shard: { cohort: string[]; selected: string[] };
	admission: string;
	engine: string;
	cases: ReportCase[];
	views: ReportView[];
	failures: string[];
	recovery?: Recovery;
};

const viewKey = (view: ReportView) => `${view.name}\0${view.width}\0${view.mode}`;
const caseKey = (view: Pick<ReportCase, "name" | "width">) => `${view.name}\0${view.width}`;

/** Merge only a fully completed cohort already admitted by the generic shard verifier. */
export function mergeNoJsShards(children: ReportShard[], { cohort, engine }: { cohort: readonly string[]; engine: string }) {
	const first = children[0], cases: ReportCase[] = [], views: ReportView[] = [], failures: string[] = [];
	requireThat(same(first.shard.cohort, cohort) && cohort.includes(SALES), "No-JS shards do not cover the complete report inventory");
	let recovery: Recovery | undefined;
	for (const child of children) {
		requireThat(child.admission === "no-js" && child.engine === engine, "No-JS shard lacks matching parent admission or engine");
		requireThat(Array.isArray(child.failures) && child.failures.every(value => typeof value === "string"), "No-JS assertion failures are missing");
		const expectedCases = child.shard.selected.flatMap(name => WIDTHS.map(width => caseKey({ name, width }))).sort();
		const expectedViews = child.shard.selected.flatMap(name => WIDTHS.flatMap(width => MODES.map(mode => viewKey({ name, width, mode })))).sort();
		requireThat(Array.isArray(child.cases) && child.cases.every(item => child.shard.selected.includes(item.name) && WIDTHS.includes(item.width) && Number.isSafeInteger(item.figures) && item.figures >= 0) && same(child.cases.map(caseKey).sort(), expectedCases), "No-JS report cases omit or repeat a width");
		requireThat(Array.isArray(child.views) && child.views.every(item => child.shard.selected.includes(item.name) && WIDTHS.includes(item.width) && MODES.includes(item.mode)) && same(child.views.map(viewKey).sort(), expectedViews), "No-JS reports did not finish every mode at both widths");
		if (child.shard.selected.includes(SALES)) {
			requireThat(!recovery && typeof child.recovery?.missing === "string" && typeof child.recovery?.present === "string", "No-JS Sales recovery drill is missing or repeated");
			recovery = child.recovery;
		} else requireThat(child.recovery === undefined, "No-JS recovery drill belongs only to the Sales report shard");
		cases.push(...child.cases); views.push(...child.views); failures.push(...child.failures);
	}
	requireThat(recovery?.missing === "The report runtime did not start" && recovery.present === "passed", "No-JS recovery drill did not fail and recover as required");
	const order = new Map(cohort.map((name, index) => [name, index]));
	const compare = (a: Pick<ReportCase, "name" | "width">, b: Pick<ReportCase, "name" | "width">) => WIDTHS.indexOf(a.width) - WIDTHS.indexOf(b.width) || order.get(a.name)! - order.get(b.name)!;
	cases.sort(compare); views.sort((a, b) => compare(a, b) || MODES.indexOf(a.mode) - MODES.indexOf(b.mode));
	return { engine, cases, failures, admission: "no-js", shard: { index: 0, count: 1, cohort, selected: cohort }, completed: cohort, views, recovery };
}
