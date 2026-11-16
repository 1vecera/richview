// Host reason (ARC-03): real independent readers exercise retained-build lifetime and immutable identity.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { keepReleaseBuild, leaseReleaseBuild } from "../lib/target.mjs";

const commits = "abcdef".split("").map(letter => letter.repeat(40));
function fixture(t) {
	const root = mkdtempSync(join(tmpdir(), "rv-kept-lease-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const options = { dir: join(root, "kept"), target: join(root, "target"), cwd: join(root, "checkout"), keep: 1 };
	mkdirSync(join(options.target, "release"), { recursive: true });
	mkdirSync(join(options.cwd, "dist/web"), { recursive: true });
	let order = 0;
	const keep = commit => {
		writeFileSync(join(options.target, "release/richview"), commit);
		writeFileSync(join(options.target, "release/richview-build-manifest.json"), JSON.stringify({ inputs: { commit } }));
		writeFileSync(join(options.cwd, "dist/web/index.html"), `<p>${commit}</p>`);
		const folder = keepReleaseBuild({ ...options, commit });
		const at = new Date(1_700_000_000_000 + ++order * 1000);
		utimesSync(folder, at, at);
		return folder;
	};
	return { ...options, keep };
}

async function reader(t, dir, commit) {
	const module = new URL("../lib/target.mjs", import.meta.url).href;
	const source = `import {leaseReleaseBuild} from ${JSON.stringify(module)};
import {readFileSync,writeFileSync} from 'node:fs';
const lease=leaseReleaseBuild({dir:process.argv[1],commit:process.argv[2]});
process.on('message',message=>{
 if(message==='read')process.send({bytes:readFileSync(lease.folder+'/richview','utf8')});
 if(message==='release'){lease.release();lease.release();process.send({released:true});}
 if(message==='guard'){writeFileSync(process.argv[1]+'/.rv-kept-build.lock',process.pid+'\\n',{flag:'wx'});process.send({guarded:true});}
});
process.send({folder:lease.folder});`;
	const child = spawn(process.execPath, ["--input-type=module", "-e", source, dir, commit], { stdio: ["ignore", "ignore", "pipe", "ipc"] });
	let stderr = "";
	child.stderr.on("data", bytes => { stderr += bytes; });
	const ended = once(child, "exit");
	const receive = () => Promise.race([once(child, "message").then(([message]) => message), ended.then(() => { throw new Error(`Lease reader exited: ${stderr}`); })]);
	t.after(async () => { if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL"); await ended; });
	const first = await receive();
	return { child, folder: first.folder, async send(message) { const reply = receive(); child.send(message); return reply; }, async stop() { child.kill("SIGKILL"); await ended; } };
}

test("independent live readers survive pruning and replacement; only the last exited owner permits cleanup", async t => {
	const f = fixture(t), folder = f.keep(commits[0]);
	const one = await reader(t, f.dir, commits[0]), two = await reader(t, f.dir, commits[0]);
	f.keep(commits[1]); f.keep(commits[2]);
	assert.ok(existsSync(join(folder, "richview")), "pruning deleted a live reader's bundle");
	assert.equal(readFileSync(join(folder, "richview"), "utf8"), commits[0], "pruning must preserve the actual live reader's bytes");
	assert.equal((await one.send("read")).bytes, commits[0]);
	assert.equal((await two.send("read")).bytes, commits[0]);
	assert.throws(() => f.keep(commits[0]), /Cannot replace a live leased build/);
	assert.equal(readFileSync(join(folder, "richview"), "utf8"), commits[0]);
	assert.deepEqual(await one.send("release"), { released: true });
	f.keep(commits[3]);
	assert.equal((await two.send("read")).bytes, commits[0], "one reader releasing cannot release the other");
	await two.stop();
	assert.throws(() => process.kill(two.child.pid, 0), { code: "ESRCH" });
	f.keep(commits[4]);
	assert.equal(existsSync(folder), false, "an abandoned lease is reclaimed after its real owner exits");
	assert.ok(existsSync(join(f.dir, commits[4].slice(0, 10), "richview")));
});

test("lease admission binds the full commit and release detects manifest replacement", t => {
	const f = fixture(t), folder = f.keep(commits[0]);
	assert.throws(() => leaseReleaseBuild({ dir: f.dir, commit: commits[0].slice(0, 10) }), /full commit/);
	const collision = commits[0].slice(0, 10) + commits[1].slice(10);
	assert.throws(() => leaseReleaseBuild({ dir: f.dir, commit: collision }), /expected full commit/);
	const lease = leaseReleaseBuild({ dir: f.dir, commit: commits[0] });
	writeFileSync(join(folder, "build-manifest.json"), JSON.stringify({ inputs: { commit: commits[0] }, changed: true }));
	assert.throws(() => f.keep(commits[1]), /live leased build changed identity/);
	assert.equal(existsSync(join(f.dir, commits[1].slice(0, 10))), false, "a broken binding must fail before touching another build");
	assert.throws(lease.release, /leased build changed identity/);
	lease.release();
});

test("corrupt lease metadata refuses retention, while a confirmed different process start cannot pin a bundle", t => {
	const f = fixture(t), folder = f.keep(commits[0]);
	leaseReleaseBuild({ dir: f.dir, commit: commits[0] });
	const leases = join(f.dir, ".rv-build-leases", commits[0].slice(0, 10));
	const file = join(leases, readdirSync(leases).find(name => name.endsWith(".json")));
	const record = JSON.parse(readFileSync(file));
	writeFileSync(file, "{");
	assert.throws(() => f.keep(commits[1]), SyntaxError);
	assert.equal(readFileSync(join(folder, "richview"), "utf8"), commits[0]);
	writeFileSync(file, JSON.stringify({ ...record, started: "a different kernel process start" }));
	f.keep(commits[1]);
	assert.equal(existsSync(folder), false, "PID reuse must not inherit the prior process's lease");
});

test("a displaced release cannot remove an identically named replacement lease", t => {
	const f = fixture(t); f.keep(commits[0]);
	const lease = leaseReleaseBuild({ dir: f.dir, commit: commits[0] });
	const leases = join(f.dir, ".rv-build-leases", commits[0].slice(0, 10));
	const file = join(leases, readdirSync(leases).find(name => name.endsWith(".json")));
	const bytes = readFileSync(file);
	writeFileSync(file + ".replacement", bytes);
	renameSync(file + ".replacement", file);
	assert.throws(lease.release, /refusing to remove its replacement/);
	assert.deepEqual(readFileSync(file), bytes);
});

test("an unavailable process-start lookup preserves existing readers and refuses a new lease", t => {
	const f = fixture(t), folder = f.keep(commits[0]);
	const lease = leaseReleaseBuild({ dir: f.dir, commit: commits[0] });
	const bin = join(f.dir, "unavailable-ps"); mkdirSync(bin);
	writeFileSync(join(bin, "ps"), "#!/bin/sh\nexit 1\n"); chmodSync(join(bin, "ps"), 0o755);
	const before = process.env.PATH;
	try {
		process.env.PATH = bin;
		f.keep(commits[1]);
		assert.ok(existsSync(join(folder, "richview")), "an unavailable owner lookup must preserve the live reader");
		assert.equal(readFileSync(join(folder, "richview"), "utf8"), commits[0]);
		assert.throws(() => leaseReleaseBuild({ dir: f.dir, commit: commits[1] }), /Cannot identify.*lease owner/);
	} finally { process.env.PATH = before; }
	lease.release();
});

test("a live mutation guard refuses both lease and prune without effects, then a dead owner is reclaimed", async t => {
	const f = fixture(t), folder = f.keep(commits[0]);
	const owner = await reader(t, f.dir, commits[0]);
	assert.deepEqual(await owner.send("guard"), { guarded: true });
	assert.throws(() => f.keep(commits[1]), /retention is busy/);
	assert.throws(() => leaseReleaseBuild({ dir: f.dir, commit: commits[0] }), /retention is busy/);
	assert.equal(existsSync(join(f.dir, commits[1].slice(0, 10))), false);
	assert.equal(readFileSync(join(folder, "richview"), "utf8"), commits[0]);
	await owner.stop();
	assert.throws(() => process.kill(owner.child.pid, 0), { code: "ESRCH" });
	f.keep(commits[1]);
	assert.equal(existsSync(folder), false);
	assert.ok(existsSync(join(f.dir, commits[1].slice(0, 10), "richview")));
});
