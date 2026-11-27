// Host reason (ARC-03): enumerate the filesystem report inputs shared by producers and admission.
import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";

const NEEDS_CONNECTOR = new Set(["sql-report", "parquet-report", "metricflow-report"]);

/** Eligible examples in their existing order, followed by explicitly named extra fixtures. */
export function reportCohort(root: string, { only, extra = ["forms"] }: { only?: readonly string[]; extra?: readonly string[] } = {}) {
	const examples = readdirSync(join(root, "examples")).sort().filter(name => !NEEDS_CONNECTOR.has(name) && existsSync(join(root, "examples", name, "report.rmd")) && (!only || only.includes(name)));
	return [...examples, ...extra];
}
