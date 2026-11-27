// Host reason (ARC-03): static checks of the release kit's bash and node steps (tools/rv-release/steps), which cannot run without
// credentials and outward effects. They keep the kit consistent with parts.json: every step exists, parses, and only waits for steps
// that its own part, a part it needs, or the shared binding runs first (DEL-83: a part never depends on a part it is independent of).
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import type { Part, Step } from "../lib/parts.mts";
import { loadParts, STAGES } from "../lib/parts.mts";

const ROOT = new URL("..", import.meta.url).pathname;
const STEPS = join(ROOT, "steps");
const model = loadParts();

function walk(dir: string): string[] {
	return readdirSync(dir).flatMap((name) => {
		const path = join(dir, name);
		return statSync(path).isDirectory() ? walk(path) : [path];
	});
}
const files = walk(STEPS);
const isBash = (path: string) => path.endsWith(".sh") || /\/(run|record-word|retain-full)$/.test(path);

test("every bash file in the kit parses", () => {
	const bash = files.filter(isBash);
	assert.ok(bash.length > 40, "the kit's bash files are present");
	for (const file of bash) execFileSync("bash", ["-n", file], { stdio: "pipe" });
});

test("every node file in the kit parses", () => {
	for (const file of files.filter((f) => /\.[mc][jt]s$/.test(f))) execFileSync(process.execPath, ["--check", file], { stdio: "pipe" });
});

/** The ordered steps of one part: `{ step, args }` over its stages, in the order the command runs them. */
const ordered = (part: Part) => STAGES.flatMap((stage) => (part[stage] ?? []).filter((s): s is Step => Boolean(s.step)).map((s) => ({ ...s, stage })));

test("every step of parts.json is a script in the kit, and the rollback steps too", () => {
	for (const part of model.parts) {
		for (const item of [...ordered(part), ...(part.rollback?.steps ?? [])]) {
			assert.ok(files.includes(join(STEPS, `${item.step}.sh`)), `${part.id}: steps/${item.step}.sh is missing`);
		}
	}
});

test("a step waits only for steps its own part, a part it needs, or the shared binding runs first", () => {
	const owners = new Map<string, {part: string; index: number}[]>();
	for (const part of model.parts) ordered(part).forEach((item, index) => owners.set(item.step, [...(owners.get(item.step) ?? []), { part: part.id, index }]));
	const needed = (id: string, seen = new Set<string>()) => {
		for (const need of model.byId.get(id)!.needs) if (!seen.has(need)) { seen.add(need); needed(need, seen); }
		return seen;
	};
	let checked = 0;
	// One step can serve two parts with different arguments (11-windows: "site" for the walk, "bonus" for the public build), so a step's
	// prerequisites may come from any part that runs it; a step that only one part runs is held to that part.
	const steps = new Map<string, Part[]>();
	for (const part of model.parts) for (const item of ordered(part)) steps.set(item.step, [...(steps.get(item.step) ?? []), part]);
	for (const [step, parts] of steps) {
		const allowed = new Set(["shared"]);
		for (const part of parts) for (const id of [part.id, ...needed(part.id), ...(part.closingAfter ?? [])]) allowed.add(id);
		const text = readFileSync(join(STEPS, `${step}.sh`), "utf8").split("\n").filter((line) => !line.trimStart().startsWith("#")).join("\n");
		for (const [, after] of text.matchAll(/(?:^|[;{(]|\bthen|\bdo)\s*after ([0-9a-z]+(?:-[0-9a-z]+)*)/gm)) {
			checked += 1;
			assert.ok(files.includes(join(STEPS, `${after}.sh`)), `${step}: after ${after} names no step`);
			const where = owners.get(after);
			if (!where) continue; // an internal sub-step of a part's own step (the mail window runs 51 to 59)
			assert.ok(where.some((w) => allowed.has(w.part)), `${step} waits for ${after}, which belongs to ${where.map((w) => w.part).join("/")}: not its part, a part it needs, or shared`);
			for (const part of parts.filter((p) => where.some((w) => w.part === p.id) && !p.closingAfter?.includes(p.id))) {
				const here = ordered(part).findIndex((x) => x.step === step);
				if (!where.some((w) => w.part !== part.id)) assert.ok(where.some((w) => w.part === part.id && w.index < here), `${part.id}/${step} waits for ${after}, which runs later in the same part`);
			}
		}
	}
	assert.ok(checked > 20, `the lint saw the kit's after-checks (${checked})`);
});

test("a step before the deploys never needs a deploy to have happened", () => {
	// Every prepare and checks step of every part runs before any part deploys (DEL-83), so none may require the prod merge, which only the
	// prod part's deploy makes true: private prod at FINAL (check_prod, ref prod = FINAL) or the prod-merge receipt. 49b-preflight checks them.
	const needsMerge = /check_prod|ref prod\)" = "\$FINAL"|prod-merge\.json|\bafter (14-private-prod|41-deploy|42-verify)\b/;
	let seen = 0;
	for (const part of model.parts) {
		for (const stage of ["prepare", "checks"] as const) {
			for (const item of (part[stage] ?? []).filter((s): s is Step => Boolean(s.step))) {
				seen += 1;
				const text = readFileSync(join(STEPS, `${item.step}.sh`), "utf8").split("\n").filter((line) => !line.trimStart().startsWith("#")).join("\n");
				assert.doesNotMatch(text, needsMerge, `${part.id}/${stage}/${item.step} needs the prod merge, but runs before every deploy`);
			}
		}
	}
	assert.ok(seen >= 10, "the lint saw the prepare and checks steps");
});

test("no step is shared by a part that could run without the part it needs", () => {
	// The independence rule (DEL-83): richview.ai and app.richview.ai share the prod merge and nothing else.
	const site = model.byId.get("site")!, hosted = model.byId.get("hosted")!;
	assert.ok(!site.needs.includes("hosted") && !hosted.needs.includes("site"), "site and hosted do not need each other");
	assert.ok(!site.closingAfter && hosted.closingAfter?.every((id) => id === "public"), "only the sign-up reopening waits, and only for the public part when it is chosen");
});

test("the kit holds no pins of one release", () => {
	// The release's own values live in its descriptor (release.json). Named long-lived approvals remain: the provider section
	// approved with release 29 (check-privacy.mjs, provider-check.sh) and the Windows tools of release 27 (a default of lib.sh).
	const forbidden = /2c220f59|3d62257a|091eac49|candidate\/3\d|release-3\d|\bv3\.[01]\.\d|vol-0[0-9a-f]{6}|snap-0[0-9a-f]{6}|ffbd972b|e742a801|14bcb9b1|46d7631b|867765c1/;
	for (const file of files) {
		const hit = readFileSync(file, "utf8").split("\n").findIndex((line) => forbidden.test(line));
		assert.equal(hit, -1, `${file.slice(ROOT.length)} line ${hit + 1} pins a release`);
	}
});
