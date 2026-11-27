// Host reason (ARC-03): tests of the release runner's order, its go gate, its checks before the deploy and the independence of its parts (DEL-83).
import test from "node:test";
import assert from "node:assert/strict";
import { runRelease } from "../lib/run.mts";
import { harness } from "./harness.mts";

import type { Runner } from "../lib/run.mts";
const run = (h: ReturnType<typeof harness>) => runRelease({ model: h.model, desc: h.desc, chosen: h.parts, journal: h.journal, runner: h.runner, ctx: h.ctx, log: h.log });
const status = (states: Awaited<ReturnType<typeof runRelease>>) => Object.fromEntries([...states].map(([id, s]) => [id, s.status]));

test("the parts run in their fixed order, prepare and checks before any deploy, sign-up reopening last", async () => {
	const h = harness();
	const states = await run(h);
	assert.deepEqual(h.calls, [
		"shared:prepare:00-bind-full full-20261125T152308Z-2c220f5929",
		"shared:prepare:01-select-parts windows,prod,site,hosted,public 2026-11-26",
		"windows:prepare:02-select-route",
		"windows:prepare:10-cut B",
		"windows:prepare:11-windows site",
		"windows:prepare:12-record-walk",
		"prod:prepare:13-packet",
		"build:prepare:15-seed",
		"build:prepare:16-macos",
		"build:prepare:17-linux",
		"public:prepare:11-windows bonus",
		"site:checks:40-dry-run",
		"hosted:checks:49-archive",
		"public:checks:18-stage",
		"public:checks:19-kit",
		"prod:deploy:14-private-prod",
		"site:deploy:41-deploy",
		"site:deploy:42-verify",
		"hosted:deploy:49b-preflight",
		"hosted:deploy:50-mail-window",
		"public:deploy:20-seed",
		"public:deploy:21-push",
		"public:deploy:29-release-plan",
		"public:deploy:30-release",
		"public:deploy:31-downloads",
		"public:deploy:32-installers",
		"public:deploy:32w-public-walk",
		"public:deploy:33-windows",
		"hosted:closing:reopen-signup-after-privacy",
	]);
	assert.deepEqual(status(states), { shared: "passed", windows: "passed", prod: "passed", build: "passed", site: "passed", hosted: "passed", public: "passed" });
	for (const id of ["windows", "prod", "site", "hosted", "public"]) assert.equal(h.journal.readPart(id)!.status, "passed", `${id} has its own journal file`);
});

test("a part left out by the go is excluded and never runs; the others still do", async () => {
	const h = harness({ chosen: ["windows", "prod", "site", "hosted"] });
	const states = await run(h);
	assert.equal(states.get("public")!.status, "excluded");
	assert.equal(states.get("build")!.status, "passed");
	assert.ok(!h.calls.some((c) => c.startsWith("public:")));
	assert.equal(states.get("hosted")!.status, "passed", "sign-up reopens when public was left out");
});

test("nothing is spent or deployed before a go covers the part", async () => {
	const h = harness({ go: false });
	const states = await run(h);
	assert.deepEqual(h.calls, ["shared:prepare:00-bind-full full-20261125T152308Z-2c220f5929", "shared:prepare:01-select-parts windows,prod,site,hosted,public 2026-11-26"]);
	assert.equal(states.get("windows")!.status, "failed-unchanged");
	assert.match(states.get("windows")!.reason, /No go covers the windows part/);
	assert.equal(states.get("site")!.status, "skipped");
});

test("a failing check before the deploy refuses richview.ai only: the hosted and public parts still run", async () => {
	const h = harness({ builtin: { "site-edge": async () => ({ status: "fail", detail: "serve /docs/example/Rendered/publisher.html answered 307" }) } });
	const states = await run(h);
	assert.equal(states.get("site")!.status, "refused");
	assert.match(states.get("site")!.reason, /publisher\.html answered 307/);
	assert.ok(!h.calls.some((c) => c.startsWith("site:deploy")), "the site was never deployed");
	assert.deepEqual([states.get("prod")!.status, states.get("hosted")!.status, states.get("public")!.status], ["passed", "passed", "passed"]);
});

test("a failing site deploy rolls back the Worker by itself and leaves the hosted and public parts running", async () => {
	const h = harness({ failing: ["41-deploy"] });
	// The recovery step records which version the failed deploy left live.
	const original = h.runner.step;
	h.runner.step = (call) => { const result = original(call); if (call.step === "41-deploy") h.journal.save("NEW_WORKER", "11111111-2222-3333-4444-555555555555"); return result; };
	const states = await run(h);
	assert.equal(states.get("site")!.status, "failed-rolled-back");
	assert.deepEqual(h.calls.filter((c) => c.startsWith("site:rollback")), ["site:rollback:recover-worker-history", "site:rollback:90-rollback 11111111-2222-3333-4444-555555555555"]);
	assert.equal(states.get("site")!.rollback!.ran!.basis!.startsWith("DEL-48"), true);
	assert.deepEqual([states.get("hosted")!.status, states.get("public")!.status], ["passed", "passed"]);
});

test("a deploy that changed nothing needs no rollback", async () => {
	const h = harness({ failing: ["41-deploy"] });
	h.ctx.newWorker = () => null;
	const states = await run(h);
	assert.equal(states.get("site")!.status, "failed-unchanged");
	assert.ok(!h.calls.some((c) => c.startsWith("site:rollback:90-rollback")));
});

test("a failing hosted update stays in its part, keeps its rollback ready and waits for Daniel's words", async () => {
	const h = harness({ failing: ["50-mail-window"] });
	const states = await run(h);
	const hosted = states.get("hosted")!;
	assert.equal(hosted.status, "failed-left");
	assert.match(hosted.reason, /recover-mail-window/);
	assert.equal(hosted.rollback!.automatic, false);
	assert.ok(!h.calls.some((c) => c.includes("recover-")), "no recovery step runs without his words");
	assert.deepEqual([states.get("site")!.status, states.get("public")!.status], ["passed", "passed"]);
	assert.ok(!h.calls.includes("hosted:closing:reopen-signup-after-privacy"));
});

test("a failing public release holds the sign-up reopen but does not undo the hosted update", async () => {
	const h = harness({ failing: ["30-release"] });
	const states = await run(h);
	assert.equal(states.get("public")!.status, "failed-left");
	assert.equal(states.get("hosted")!.status, "held");
	assert.match(states.get("hosted")!.reason, /public/);
	assert.equal(states.get("site")!.status, "passed");
	assert.ok(!h.calls.includes("hosted:closing:reopen-signup-after-privacy"));
});

test("a failing prod merge skips everything that needs production and runs the rest", async () => {
	const h = harness({ failing: ["14-private-prod"] });
	const states = await run(h);
	assert.equal(states.get("prod")!.status, "failed-left");
	for (const id of ["site", "hosted", "public"]) assert.equal(states.get(id)!.status, "skipped", id);
	assert.match(states.get("site")!.reason, /prod/);
	assert.ok(!h.calls.some((c) => /deploy:(41|49b|20)/.test(c)));
});

test("a failing Windows walk skips production and the build that nothing can use", async () => {
	const h = harness({ failing: ["11-windows site"] });
	const states = await run(h);
	assert.equal(states.get("windows")!.status, "failed-unchanged");
	for (const id of ["prod", "site", "hosted", "public"]) assert.equal(states.get(id)!.status, "skipped", id);
	assert.equal(states.get("build")!.status, "skipped");
	assert.ok(!h.calls.some((c) => c.startsWith("build:")));
});

test("a failing build refuses the hosted and public parts and leaves richview.ai alone", async () => {
	const h = harness({ failing: ["17-linux"] });
	const states = await run(h);
	assert.equal(states.get("build")!.status, "failed-unchanged");
	assert.deepEqual([states.get("hosted")!.status, states.get("public")!.status], ["skipped", "skipped"]);
	assert.deepEqual([states.get("prod")!.status, states.get("site")!.status], ["passed", "passed"]);
});

test("a part whose rollback is not ready never starts its deploy", async () => {
	const h = harness();
	const original = h.runner.step;
	h.runner.step = (call) => call.step === "40-dry-run" ? { code: 0 } : original(call); // 40 passes without saving BEFORE_WORKER
	const states = await run(h);
	assert.equal(states.get("site")!.status, "refused");
	assert.match(states.get("site")!.reason, /no rollback ready: the journal lacks BEFORE_WORKER, DRY_SHA/);
	assert.ok(!h.calls.includes("site:deploy:41-deploy"));
});

test("evidence that changes after the go was bound refuses that part", async () => {
	const h = harness();
	const original = h.runner.step;
	// Bind the site part's evidence after its dry run, then change the dry run's digest once the prod part ran.
	const { verifiedGo } = await import("../lib/go.mts");
	h.runner.step = (call) => { const r = original(call); if (call.step === "40-dry-run") verifiedGo(h.journal, "site", h.desc); if (call.step === "14-private-prod") h.journal.save("DRY_SHA", "9".repeat(64)); return r; };
	const states = await run(h);
	assert.equal(states.get("site")!.status, "refused");
	assert.match(states.get("site")!.reason, /evidence bound to the go for site has changed/);
});

test("a second run resumes: parts that passed and steps that are done are not run again", async () => {
	const h = harness({ failing: ["30-release"] });
	await run(h);
	h.calls.length = 0;
	const h2 = { ...h, runner: { step: (call: Parameters<Runner["step"]>[0]) => { h.calls.push(`${call.part}:${call.stage}:${call.step}`); h.journal.save(`ok-${call.step}`, "yes"); return { code: 0 }; } } };
	const states = await run(h2);
	assert.equal(states.get("site")!.status, "passed");
	assert.ok(!h.calls.some((c) => c.startsWith("site:") || c.startsWith("prod:")), `unexpected re-run: ${h.calls.join(", ")}`);
	assert.ok(h.calls.some((c) => c.includes("public:deploy:30-release")));
});
