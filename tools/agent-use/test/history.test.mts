import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { appendHistory, readHistory } from "../history.mts";
import { render, series } from "../report.mts";

import type { HistoryRow } from "../history.mts";

const run = (at: string, score: number, commit: string, extra: Partial<HistoryRow> = {}) => ({ at, task: "t", product: "claude", model: "sonnet", build: { commit }, score, pass: score > 80, firstCleanAttempt: 1, toolCalls: 9, facts: { passed: 3, total: 3, failed: [] }, misread: [], ...extra });

test("history lines round-trip and the report flags a drop against recent runs", () => {
	const dir = mkdtempSync(join(tmpdir(), "rv-agent-use-hist-"));
	try {
		const file = join(dir, "h", "history.jsonl");
		for (const r of [run("2026-10-01T10:00", 95, "aaa"), run("2026-10-08T10:00", 90, "aaa"), run("2026-10-15T10:00", 60, "bbb", { misread: ["chart.failed"] })]) appendHistory(r, file);
		const groups = series(readHistory(file));
		assert.equal(groups.length, 1);
		const last = groups[0].runs.at(-1)!;
		assert.equal(last.flagged, true);
		assert.equal(last.buildChanged, true);
		assert.equal(groups[0].runs[1].flagged, false);
		assert.match(render(groups), /DROP; new build; misread chart\.failed/);
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});
