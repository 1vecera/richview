// Host reason (ARC-03): exercise Cargo test orchestration with disposable executable fixtures.
import assert from "node:assert/strict";
import test from "node:test";
import { existsSync, mkdtempSync, mkdirSync, writeFileSync, readFileSync, realpathSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { runRustTests } from "../lib/rust-tests.mts";
import { dropTestBinaries } from "../lib/target.mts";

type RustFixtureEvent = { kind: string; name: string; pid: number; args: string[] };
type ChildClose = { code: number | null; signal: NodeJS.Signals | null };
const readEvents = (file: string) => readFileSync(file, "utf8").trim().split("\n").map((line): RustFixtureEvent => JSON.parse(line));
const readCalls = (file: string) => readFileSync(file, "utf8").trim().split("\n").map((line): string[] => JSON.parse(line));

function fixture(t: test.TestContext, mode = "pass") {
	const root = realpathSync(mkdtempSync(join(tmpdir(), "rv-rust-runner-")));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const bin = join(root, "bin"), target = join(root, "target"), pkg = join(root, "package");
	for (const path of [bin, target, pkg, join(target, "debug/deps"), join(target, "debug/examples")]) mkdirSync(path, { recursive: true });
	writeFileSync(join(target, ".rv-gate-cargo"), `${process.pid}\n`);
	const manifest = join(pkg, "Cargo.toml");
	writeFileSync(manifest, "fixture");
	const executable = (name: string, source: string) => { const path = join(bin, name); writeFileSync(path, `#!${process.execPath}\n${source}\n`, { mode: 0o755 }); return path; };
	const targets: { name: string; kind: string[]; test: boolean; "required-features"?: string[] }[] = ["a", "b", "c"].map(name => ({ name, kind: [name === "c" ? "example" : "test"], test: true }));
	if (mode === "required-feature") targets[0]["required-features"] = ["optional"];
	const library = { name: "fixture", kind: ["lib"], test: false, doctest: true };
	const metadata = { workspace_members: ["fixture"], target_directory: target, packages: [{ id: "fixture", name: "fixture", version: "1.2.3-pre", authors: ["One", "Two"], manifest_path: manifest, targets: [...targets, library] }] };
	const artifacts = targets.map(item => ({ reason: "compiler-artifact", package_id: "fixture", manifest_path: manifest, profile: { test: true }, target: item, executable: join(target, item.kind.includes("example") ? "debug/examples" : "debug/deps", item.name) }));
	for (const artifact of artifacts) writeFileSync(artifact.executable, `#!${process.execPath}\nimport{appendFileSync,readFileSync}from'node:fs';
if(process.cwd()!==${JSON.stringify(pkg)}||process.env.CARGO_MANIFEST_DIR!==${JSON.stringify(pkg)}||process.env.CARGO_PKG_VERSION!=='1.2.3-pre'||process.env.CARGO_PKG_AUTHORS!=='One:Two')process.exit(9);
if(!(process.env[process.platform==='darwin'?'DYLD_FALLBACK_LIBRARY_PATH':'LD_LIBRARY_PATH']??'').split(':').includes(${JSON.stringify(join(target, "debug/deps"))})){console.log('missing native dependency library path');process.exit(11);}
appendFileSync(${JSON.stringify(join(root, "events"))},JSON.stringify({kind:'start',pid:process.pid,name:${JSON.stringify(artifact.target.name)},args:process.argv.slice(2)})+'\\n');
if(${JSON.stringify(mode)}==='wait')await new Promise(r=>setTimeout(r,30000));
console.log('running 2 tests');
const deadline=Date.now()+5000;
while(readFileSync(${JSON.stringify(join(root, "events"))},'utf8').split('"kind":"start"').length<3){if(Date.now()>deadline)throw Error('second worker did not start');await new Promise(r=>setTimeout(r,5));}
console.log('test actual_case ... ok');console.log('test staged_case ... ignored, requires staged runtime');
appendFileSync(${JSON.stringify(join(root, "events"))},JSON.stringify({kind:'end',name:${JSON.stringify(artifact.target.name)}})+'\\n');
console.log('test result: '+(${JSON.stringify(mode)}==='child-fail'&&${JSON.stringify(artifact.target.name)}==='a'?'FAILED. 0 passed; 1 failed':'ok. 1 passed; 0 failed')+'; 1 ignored; 0 measured; 0 filtered out; finished in 0.04s');
process.exit(${JSON.stringify(mode)}==='child-fail'&&${JSON.stringify(artifact.target.name)}==='a'?1:0);\n`, { mode: 0o755 });
	executable("cargo", `import{appendFileSync}from'node:fs';
const args=process.argv.slice(2);appendFileSync(${JSON.stringify(join(root, "calls"))},JSON.stringify(args)+'\\n');
if(args[0]==='metadata')console.log(${JSON.stringify(JSON.stringify(metadata))});
else if(args.includes('--no-run')){for(const row of ${JSON.stringify(mode === "missing-cohort" ? artifacts.slice(1) : artifacts)})console.log(JSON.stringify(row));console.log(JSON.stringify({reason:'build-finished',success:true}));}
else if(args.includes('--doc')){if(${JSON.stringify(mode)}!=='missing-doc')console.log('   Doc-tests fixture');console.log('test doc_case ... '+(${JSON.stringify(mode)}==='doc-fail'?'FAILED':'ok'));process.exit(${JSON.stringify(mode)}==='doc-fail'?1:0);}
else {console.log('legacy command');process.exit(0);}`);
	executable("rustc", `console.log(${JSON.stringify(join(root, "sysroot"))});`);
	const env: NodeJS.ProcessEnv = { ...process.env, PATH: bin + ":" + process.env.PATH, CARGO_TARGET_DIR: target, CARGO_HOME: join(root, "cargo-home") };
	for (const key of Object.keys(env)) if (key === "CARGO_BUILD_TARGET" || /^CARGO_TARGET_.+_RUNNER$/.test(key)) delete env[key];
	return { root, target, env, cwd: root, logPath: join(root, "test.log"), args: ["test", "--workspace", "--locked", "--no-fail-fast"] };
}

test("all Cargo-selected binaries run in a bounded pool with package context, original filters and doctests", async t => {
	const f = fixture(t);
	f.args.push("--features", "fixture/runtime", "--", "--ignored", "--skip", "external_case", "--exact");
	const result = await runRustTests({ ...f, signal: undefined });
	assert.equal(result.status, "pass", readFileSync(f.logPath, "utf8"));
	const calls = readCalls(join(f.root, "calls"));
	assert.ok(calls.some(args => args.includes("--no-run") && args.includes("fixture/runtime") && !args.includes("--ignored")));
	assert.ok(calls.some(args => args.includes("--doc") && args.slice(-5).join(" ") === "-- --ignored --skip external_case --exact"));
	const events = readEvents(join(f.root, "events"));
	let active = 0, peak = 0;
	for (const event of events) { active += event.kind === "start" ? 1 : -1; peak = Math.max(peak, active); if (event.kind === "start") assert.deepEqual(event.args, ["--ignored", "--skip", "external_case", "--exact"]); }
	assert.equal(peak, 2); assert.equal(active, 0);
	assert.deepEqual(events.filter(e => e.kind === "end").map(e => e.name).sort(), ["a", "b", "c"]);
	assert.equal((readFileSync(f.logPath, "utf8").match(/ignored, requires staged runtime/g) ?? []).length, 3);
});

for (const mode of ["child-fail", "doc-fail", "missing-cohort", "missing-doc"] as const) test(`${mode} cannot produce a passing Rust step`, async t => {
	const f = fixture(t, mode), result = await runRustTests({ ...f, signal: undefined });
	assert.equal(result.status, "fail");
	const log = readFileSync(f.logPath, "utf8");
	const causes = { "child-fail": /test result: FAILED/, "doc-fail": /test doc_case \.\.\. FAILED/, "missing-cohort": /Incomplete Cargo test cohort: 2\/3/, "missing-doc": /Missing doctest cohort fixture/ };
	assert.match(log, causes[mode], "the intended guard, not another error, must fail");
	if (mode === "child-fail") {
		assert.equal((log.match(/running 2 tests/g) ?? []).length, 3, "one failed child must not drop queued binaries");
		assert.match(log, /Doc-tests fixture/);
	}
});

test("a missing executable and absent Cargo hold fail before tests can claim success", async t => {
	const f = fixture(t);
	rmSync(join(f.target, "debug/deps/a"));
	assert.equal((await runRustTests({ ...f, signal: undefined })).status, "fail");
	assert.match(readFileSync(f.logPath, "utf8"), /Missing Cargo test executable/);
	rmSync(join(f.target, ".rv-gate-cargo"));
	assert.equal((await runRustTests({ ...f, signal: undefined })).status, "fail");
	assert.match(readFileSync(f.logPath, "utf8"), /requires the caller's complete Cargo\/source hold/);
});

test("other Cargo selections keep the original Cargo command", async t => {
	const f = fixture(t); f.args = ["test", "--package", "fixture", "--locked"];
	assert.equal((await runRustTests({ ...f, signal: undefined })).status, "pass");
	assert.deepEqual(readCalls(join(f.root, "calls")), [f.args]);
});

for (const mode of ["required-feature", "cargo-env"]) test(`${mode} defers the complete command to Cargo without pooling a partial cohort`, async t => {
	const f = fixture(t, mode);
	if (mode === "cargo-env") { mkdirSync(join(f.root, ".cargo")); writeFileSync(join(f.root, ".cargo/config.toml"), "[env]\nFIXTURE_VALUE = 'configured'\n"); }
	assert.equal((await runRustTests({ ...f, signal: undefined })).status, "pass");
	const calls = readCalls(join(f.root, "calls"));
	assert.deepEqual(calls.at(-1), f.args);
	assert.equal(existsSync(join(f.root, "events")), false, "custom Cargo context must not enter the native pool");
});

test("aborting drains active binary processes before returning and prevents success", { timeout: 60000 }, async t => {
	const f = fixture(t, "wait"), controller = new AbortController();
	const run = runRustTests({ ...f, signal: controller.signal });
	let starts: RustFixtureEvent[] = [];
	const deadline = Date.now() + 30000; // polls every 5 ms; the budget only has to outlast a loaded host
	while (starts.length < 2 && Date.now() < deadline) {
		try { starts = readEvents(join(f.root, "events")).filter(e => e.kind === "start"); } catch { /* children have not started */ }
		await new Promise(done => setTimeout(done, 5));
	}
	controller.abort();
	assert.equal((await run).status, "fail");
	assert.equal(starts.length, 2, "the control must cancel actual active children");
	for (const { pid } of starts) assert.throws(() => process.kill(pid, 0), error => (error as NodeJS.ErrnoException).code === "ESRCH");
	assert.match(readFileSync(f.logPath, "utf8"), /Rust cohort aborted/);
});

test("terminating the parent preserves its default stop and leaves no detached test process", { timeout: 60000 }, async t => {
	const f = fixture(t, "wait");
	const module = fileURLToPath(new URL("../lib/rust-tests.mts", import.meta.url));
	const driver = join(f.root, "driver.mjs");
	writeFileSync(driver, `import{writeFileSync}from'node:fs';import{runRustTests}from${JSON.stringify(module)};
writeFileSync(${JSON.stringify(join(f.target, ".rv-gate-cargo"))},String(process.pid));
await runRustTests({cwd:${JSON.stringify(f.cwd)},env:process.env,logPath:${JSON.stringify(f.logPath)},args:${JSON.stringify(f.args)}});`);
	const child = spawn(process.execPath, [driver], { env: f.env, stdio: "ignore" });
	const closed = new Promise<ChildClose>(resolve => child.once("close", (code, signal) => resolve({ code, signal })));
	let starts: RustFixtureEvent[] = [];
	try {
		const deadline = Date.now() + 30000; // polls every 5 ms; the budget only has to outlast a loaded host
		while (starts.length < 2 && Date.now() < deadline) {
			try { starts = readEvents(join(f.root, "events")).filter(e => e.kind === "start"); } catch { /* wait for actual children */ }
			await new Promise(done => setTimeout(done, 5));
		}
		assert.equal(starts.length, 2, "the control must terminate a parent with active tests");
		child.kill("SIGTERM");
		const result = await Promise.race([closed, new Promise<never>((_, reject) => { const timer = setTimeout(() => reject(new Error("parent did not stop")), 15000); timer.unref(); })]);
		assert.ok(result.signal === "SIGTERM" || result.code === 143);
		for (const { pid } of starts) assert.throws(() => process.kill(pid, 0), error => (error as NodeJS.ErrnoException).code === "ESRCH");
	} finally {
		if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
		for (const { pid } of starts) { try { process.kill(pid, "SIGKILL"); } catch (error) { if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error; } }
		await closed;
	}
});

test("on a short disk each binary goes once it has run, and with room every binary stays", async t => {
	for (const [free, kept] of [[10, false], [100, true]]) {
		const f = fixture(t);
		writeFileSync(join(f.root, "bin/df"), `#!${process.execPath}\nconsole.log('Filesystem 1G-blocks Used Available Capacity Mounted\\nfixture 200 1 ${free} 1% /');\n`, { mode: 0o755 });
		const path = process.env.PATH;
		process.env.PATH = `${join(f.root, "bin")}:${path}`;
		try {
			assert.equal((await runRustTests({ ...f, signal: undefined })).status, "pass", readFileSync(f.logPath, "utf8"));
		} finally {
			process.env.PATH = path;
		}
		const binaries = ["debug/deps/a", "debug/deps/b", "debug/examples/c"].map(name => join(f.target, name));
		assert.deepEqual(binaries.map(existsSync), binaries.map(() => kept), `${free} GiB free`);
		const summary = JSON.parse(readFileSync(`${f.logPath}.rust.json`, "utf8"));
		assert.equal(summary.completed, 3);
		assert.equal(summary.dropped.count, kept ? 0 : 3);
		assert.equal(/deleted 3 test binaries after they ran/.test(readFileSync(f.logPath, "utf8")), !kept);
	}
});

test("only files inside the target go, and only under 25 GiB free", t => {
	const root = realpathSync(mkdtempSync(join(tmpdir(), "rv-drop-binaries-")));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const target = join(root, "target"), deps = join(target, "debug/deps");
	mkdirSync(deps, { recursive: true });
	const inside = join(deps, "line_contract-0123456789abcdef"), outside = join(root, "line_contract-0123456789abcdef"), folder = join(deps, "build");
	writeFileSync(inside, "12345");
	writeFileSync(outside, "12345");
	mkdirSync(folder);
	const all = [inside, outside, folder, join(deps, "gone-0123456789abcdef")];
	assert.deepEqual(dropTestBinaries(target, all, 25), { count: 0, bytes: 0 });
	assert.deepEqual(dropTestBinaries(target, all, undefined), { count: 0, bytes: 0 }, "an unknown free space keeps everything");
	assert.deepEqual(dropTestBinaries(target, [...all, inside], 24), { count: 1, bytes: 5 });
	assert.deepEqual([inside, outside, folder].map(existsSync), [false, true, true]);
});

test("a terminal signal drains a Rust cohort and an ordinary command together", {timeout: 30000}, async t => {
 const f=fixture(t,"wait"), rustModule=fileURLToPath(new URL("../lib/rust-tests.mts",import.meta.url));
 const procModule=fileURLToPath(new URL("../lib/proc.mts",import.meta.url)), driver=join(f.root,"mixed-driver.mjs"), marker=join(f.root,"ordinary-pid"), complete=join(f.root,"mixed-complete");
 const ordinary=`require('node:fs').writeFileSync(process.argv[1],String(process.pid));process.on('SIGTERM',()=>setTimeout(()=>process.exit(0),150));setInterval(()=>{},1000);`;
 writeFileSync(driver,`import{writeFileSync}from'node:fs';import{runRustTests}from${JSON.stringify(rustModule)};import{runLogged}from${JSON.stringify(procModule)};
writeFileSync(${JSON.stringify(join(f.target,'.rv-gate-cargo'))},String(process.pid));
setInterval(()=>{if(process.ppid===1)process.kill(process.pid,'SIGTERM')},100).unref();
try {await Promise.all([runRustTests({cwd:${JSON.stringify(f.cwd)},env:process.env,logPath:${JSON.stringify(f.logPath)},args:${JSON.stringify(f.args)}}),runLogged(process.execPath,['-e',${JSON.stringify(ordinary)},${JSON.stringify(marker)}],{logPath:${JSON.stringify(join(f.root,'ordinary.log'))}})]);} finally {writeFileSync(${JSON.stringify(complete)},'drained');}`);
 const parent=spawn(process.execPath,[driver],{env:f.env,stdio:'ignore'}),closed=new Promise<ChildClose>(resolve=>parent.once('close',(code,signal)=>resolve({code,signal})));
 let starts: RustFixtureEvent[] = [], other: number | undefined;
 try {
  const end=Date.now()+15000;
  while(Date.now()<end) {
   try {starts=readEvents(join(f.root, "events")).filter(e=>e.kind==='start');} catch {}
   try {other=Number(readFileSync(marker,'utf8'));} catch {}
   if(starts.length>=2&&other)break;await new Promise(resolve=>setTimeout(resolve,10));
  }
  assert.equal(starts.length,2);assert.ok(other);parent.kill('SIGTERM');
  const result=await closed;assert.equal(result.signal,'SIGTERM');assert.equal(readFileSync(complete,'utf8'),'drained');
  for(const pid of [...starts.map(item=>item.pid),other]) assert.throws(()=>process.kill(pid,0),{code:'ESRCH'});
 } finally {
  if(parent.exitCode===null&&parent.signalCode===null)parent.kill('SIGTERM');
  for(const pid of [...starts.map(item=>item.pid),other].filter((pid): pid is number => Boolean(pid)))try{process.kill(-pid,'SIGTERM');}catch(error){if((error as NodeJS.ErrnoException).code!=='ESRCH')throw error;}
  for(const pid of [...starts.map(item=>item.pid),other].filter((pid): pid is number => Boolean(pid)))try{process.kill(pid,'SIGKILL');}catch(error){if((error as NodeJS.ErrnoException).code!=='ESRCH')throw error;}
  await closed;
 }
});
