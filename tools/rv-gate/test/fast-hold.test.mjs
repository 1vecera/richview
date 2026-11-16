// S27-751: real concurrent fast processes use fake Cargo, never the shared host target.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";
import { acquireFastCargo, cargoHolder } from "../lib/target.mjs";

async function until(check) {
	const end = Date.now() + 15_000;
	while (!check()) {
		if (Date.now() > end) throw new Error("fake fast process did not reach the expected state");
		await sleep(10);
	}
}

function fixture(t) {
	mkdirSync("tmp", { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/fast-hold-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const home = join(root, "home");
	const bin = join(home, "tmp/richview-rewrite/bin");
	mkdirSync(bin, { recursive: true });
	const target = join(root, "target");
	const trace = join(root, "trace.jsonl");
	const releaseA = join(root, "release-a");
	// Fake Cargo never builds: its fixture also supplies a fixed disk reading, independent of
	// the busy host's floor. Production fast runs still use the real df before and after waiting.
	writeFileSync(join(bin, "df"), "#!/usr/bin/env node\nconsole.log('Filesystem blocks used available capacity mounted\\nfixture 200 1 100 1% /');\n", { mode: 0o755 });
	writeFileSync(join(bin, "cargo"), `#!/usr/bin/env node
import fs from 'node:fs';import path from 'node:path';
const cwd=process.cwd(),command=process.argv[2],owner=path.basename(cwd);
if(command==='metadata'){
 console.log(JSON.stringify({packages:[{name:'hold-fixture',manifest_path:path.join(cwd,'member/Cargo.toml'),dependencies:[],targets:[{name:'hold_fixture',kind:['lib'],src_path:path.join(cwd,'member/src/lib.rs')}]}]}));
}else{
 const record=event=>fs.appendFileSync(process.env.FAST_TRACE,JSON.stringify({owner,command,event,pid:process.pid,profile:fs.existsSync(path.join(process.env.CARGO_TARGET_DIR,'.rv-gate-cargo.profile'))?fs.readFileSync(path.join(process.env.CARGO_TARGET_DIR,'.rv-gate-cargo.profile'),'utf8').trim():null})+'\\n');
 record('start');
 const finish=()=>setTimeout(()=>{record('end');console.log('test result: ok. 1 passed; 0 failed');},100);
 if(owner==='a'&&command==='test'){const timer=setInterval(()=>{if(fs.existsSync(process.env.FAST_RELEASE_A)){clearInterval(timer);finish();}},10);}else finish();
}
`, { mode: 0o755 });
	const fast = fileURLToPath(new URL("../lib/fast.mjs", import.meta.url));
	const runner = join(root, "runner.mjs");
	writeFileSync(runner, `import {fastGate} from ${JSON.stringify(pathToFileURL(fast).href)}; process.exitCode=await fastGate({cwd:process.argv[2],target:process.argv[3]});`);
	const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, HOME: home, RV_GATE_HOME: join(root, "gate-home"), FAST_TRACE: trace, FAST_RELEASE_A: releaseA };
	const repos = ["a", "b"].map((name) => {
		const dir = join(root, name);
		mkdirSync(join(dir, "member/src"), { recursive: true });
		writeFileSync(join(dir, "Cargo.toml"), '[workspace]\nmembers=["member"]\n');
		const manifest = join(dir, "member/Cargo.toml");
		writeFileSync(manifest, '[package]\nname="hold-fixture"\nversion="0.1.0"\nedition="2024"\n');
		writeFileSync(join(dir, "member/src/lib.rs"), "#[test]\nfn contract() {}\n");
		// A manifest edit selects gate tooling too (ARC-01). Keep that part installed and
		// dependency-free, so these locking fixtures never run npm or use the network.
		mkdirSync(join(dir, "node_modules"));
		mkdirSync(join(dir, "tools/rv-gate/test"), { recursive: true });
		writeFileSync(join(dir, ".gitignore"), "node_modules/\n");
		writeFileSync(join(dir, "package.json"), JSON.stringify({ name: "hold-fixture", private: true }));
		const lock = JSON.stringify({ lockfileVersion: 3, packages: {} });
		writeFileSync(join(dir, "package-lock.json"), lock);
		writeFileSync(join(dir, "node_modules/.package-lock.json"), lock);
		writeFileSync(join(dir, "tools/rv-gate/test/fixture.test.mjs"), "import test from 'node:test'; test('fixture tool', () => {});\n");
		const git = (...args) => execFileSync("git", ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", ...args], { cwd: dir, env, stdio: ["ignore", "pipe", "pipe"] });
		git("init", "-q"); git("add", "."); git("commit", "-qm", "base"); git("update-ref", "refs/remotes/origin/dev", "HEAD");
		writeFileSync(manifest, `${readFileSync(manifest, "utf8")}description="changed"\n`);
		git("add", "."); git("commit", "-qm", "change");
		return dir;
	});
	const children = [];
	t.after(() => { for (const child of children) if (child.exitCode === null && child.signalCode === null) child.kill("SIGTERM"); });
	const start = (index) => {
		const child = spawn(process.execPath, [runner, repos[index], target], { env, stdio: ["ignore", "pipe", "pipe"] });
		children.push(child);
		let output = "";
		child.stdout.on("data", (data) => (output += data)); child.stderr.on("data", (data) => (output += data));
		const ended = new Promise((resolve, reject) => { child.on("error", reject); child.on("close", (code) => resolve({ code, output })); });
		return { child, ended, output: () => output };
	};
	const events = () => existsSync(trace) ? readFileSync(trace, "utf8").trim().split("\n").filter(Boolean).map(JSON.parse) : [];
	return { target, start, events, allowA: () => writeFileSync(releaseA, "go") };
}

test("two concurrent fast runs keep each run's Cargo commands together", { timeout: 25_000 }, async (t) => {
	const f = fixture(t);
	const a = f.start(0);
	await until(() => {
		if (a.child.exitCode !== null) throw new Error(a.output());
		return f.events().some((event) => event.owner === "a" && event.event === "start");
	});
	const b = f.start(1);
	await until(() => b.output().includes("unit tests:"));
	await until(() => f.events().some((event) => event.owner === "b") || b.output().includes("waiting for"));
	f.allowA();
	for (const ended of await Promise.all([a.ended, b.ended])) assert.equal(ended.code, 0, ended.output);
	assert.deepEqual(f.events().filter((event) => event.event === "start").map(({ owner, command }) => [owner, command]), [["a", "test"], ["a", "clippy"], ["b", "test"], ["b", "clippy"]]);
	assert.ok(f.events().every((event) => event.profile === "debug"));
	assert.equal(existsSync(join(f.target, ".rv-gate-cargo")), false);
	assert.equal(existsSync(join(f.target, ".rv-gate-cargo.profile")), false);
	assert.equal((b.output().match(/waiting for/g) ?? []).length, 1);
});

for (const signal of ["SIGINT", "SIGTERM"]) test(`a fast run releases its hold and Cargo on ${signal}`, { timeout: 20_000 }, async (t) => {
	const f = fixture(t);
	const a = f.start(0);
	await until(() => {
		if (a.child.exitCode !== null) throw new Error(a.output());
		return f.events().some((event) => event.event === "start");
	});
	const cargoPid = f.events()[0].pid;
	a.child.kill(signal);
	const result = await a.ended;
	assert.notEqual(result.code, 0, result.output);
	assert.equal(existsSync(join(f.target, ".rv-gate-cargo")), false);
	assert.equal(existsSync(join(f.target, ".rv-gate-cargo.profile")), false);
	assert.throws(() => process.kill(cargoPid, 0), { code: "ESRCH" });
});

test("the atomic hold reclaims only dead owners and an old release cannot clear its successor", async (t) => {
	mkdirSync("tmp", { recursive: true });
	const target = mkdtempSync(join(process.cwd(), "tmp/fast-hold-owner-"));
	t.after(() => rmSync(target, { recursive: true, force: true }));
	const file = join(target, ".rv-gate-cargo");
	const gone = execFileSync(process.execPath, ["-e", "process.stdout.write(String(process.pid))"], { encoding: "utf8" });
	writeFileSync(file, `${gone}\n`);
	const first = await acquireFastCargo(target);
	assert.equal(cargoHolder(target), process.pid);
	let waiting = 0;
	const pending = acquireFastCargo(target, { every: 5, waiting: () => waiting++ });
	await until(() => waiting === 1);
	first.release();
	const second = await pending;
	first.release();
	assert.equal(cargoHolder(target), process.pid);
	assert.equal(readFileSync(`${file}.profile`, "utf8").trim(), "debug");
	second.release();
	writeFileSync(file, "not a pid\n");
	const controller = new AbortController();
	await assert.rejects(acquireFastCargo(target, { signal: controller.signal, every: 1, waiting: () => controller.abort() }), { name: "AbortError" });
	assert.equal(readFileSync(file, "utf8"), "not a pid\n");
});

test("a displaced fast run refuses the next Cargo command and clears only its own profile", { timeout: 20_000 }, async (t) => {
	const f = fixture(t);
	const a = f.start(0);
	await until(() => f.events().some((event) => event.event === "start"));
	// Model an old external actor overwriting the marker. Current gate acquisitions wait instead.
	writeFileSync(join(f.target, ".rv-gate-cargo"), `${process.pid}\n`);
	rmSync(join(f.target, ".rv-gate-cargo"));
	f.allowA();
	const result = await a.ended;
	assert.notEqual(result.code, 0, result.output);
	assert.match(result.output, /Lost the fast Cargo hold/);
	assert.deepEqual(f.events().filter((event) => event.event === "start").map((event) => event.command), ["test"]);
	assert.equal(existsSync(join(f.target, ".rv-gate-cargo.profile")), false);
});

test("simultaneous waiters reclaim a dead hold without overlap or lost ownership", { timeout: 20_000 }, async (t) => {
	mkdirSync("tmp", { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/fast-hold-reclaim-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const module = new URL("../lib/target.mjs", import.meta.url).href;
	const worker = join(root, "worker.mjs");
	writeFileSync(worker, `import fs from 'node:fs';import {setTimeout as sleep} from 'node:timers/promises';import {acquireFastCargo} from ${JSON.stringify(module)};
const [target,go,ready,log]=process.argv.slice(2);fs.writeFileSync(ready,'ready');while(!fs.existsSync(go))await sleep(1);
const hold=await acquireFastCargo(target,{every:2});const witness=target+'/inside';const fd=fs.openSync(witness,'wx');const start=process.hrtime.bigint().toString();await sleep(25);hold.assertOwned();const end=process.hrtime.bigint().toString();fs.closeSync(fd);fs.rmSync(witness);hold.release();fs.appendFileSync(log,JSON.stringify({start,end})+'\\n');`);
	for (let round = 0; round < 10; round++) {
		const target = join(root, `target-${round}`);
		mkdirSync(target);
		const dead = execFileSync(process.execPath, ["-e", "process.stdout.write(String(process.pid))"], { encoding: "utf8" });
		writeFileSync(join(target, ".rv-gate-cargo"), `${dead}\n`);
		const go = join(root, `go-${round}`);
		const log = join(root, `log-${round}`);
		const ready = [];
		const children = Array.from({ length: 12 }, (_, index) => {
			const marker = join(root, `ready-${round}-${index}`);
			ready.push(marker);
			const child = spawn(process.execPath, [worker, target, go, marker, log], { stdio: ["ignore", "pipe", "pipe"] });
			t.after(() => { if (child.exitCode === null && child.signalCode === null) child.kill("SIGTERM"); });
			let output = "";
			child.stderr.on("data", (data) => (output += data));
			return new Promise((resolve, reject) => { child.on("error", reject); child.on("close", (code) => resolve({ code, output })); });
		});
		await until(() => ready.every((marker) => existsSync(marker)));
		writeFileSync(go, "go");
		for (const result of await Promise.all(children)) assert.equal(result.code, 0, result.output);
		// The witness rejects overlap directly. Use the host monotonic clock for cross-process
		// intervals too; performance.timeOrigin is calibrated separately in each child.
		const holds = readFileSync(log, "utf8").trim().split("\n").map(JSON.parse).sort((a, b) => BigInt(a.start) < BigInt(b.start) ? -1 : 1);
		assert.equal(holds.length, 12);
		for (let index = 1; index < holds.length; index++) assert.ok(BigInt(holds[index].start) >= BigInt(holds[index - 1].end), `holders overlap: ${JSON.stringify(holds.slice(index - 1, index + 1))}`);
		for (const file of [".rv-gate-cargo", ".rv-gate-cargo.profile", ".rv-gate-cargo.reclaim"]) assert.equal(existsSync(join(target, file)), false);
	}
});

test("an abandoned empty reclaim guard cannot block recovery of a dead holder", async (t) => {
	mkdirSync("tmp", { recursive: true });
	const target = mkdtempSync(join(process.cwd(), "tmp/fast-hold-empty-guard-"));
	t.after(() => rmSync(target, { recursive: true, force: true }));
	const dead = execFileSync(process.execPath, ["-e", "process.stdout.write(String(process.pid))"], { encoding: "utf8" });
	writeFileSync(join(target, ".rv-gate-cargo"), `${dead}\n`);
	const guard = join(target, ".rv-gate-cargo.reclaim");
	writeFileSync(guard, "");
	const old = new Date(Date.now() - 20_000);
	utimesSync(guard, old, old);
	const controller = new AbortController();
	const timer = setTimeout(() => controller.abort(), 1000);
	try {
		const hold = await acquireFastCargo(target, { signal: controller.signal, every: 1 });
		hold.assertOwned();
		hold.release();
		assert.equal(existsSync(guard), false);
	} finally { clearTimeout(timer); }
});
