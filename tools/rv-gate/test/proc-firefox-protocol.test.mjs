// Host reason (ARC-03): real command transport preserves canonical output, failure and journal behavior.
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, existsSync, watch } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { gateEnv, runLogged } from "../lib/proc.mjs";

const PRIVATE = "PRIVATE_PROTOCOL_SOURCE_NEVER_PERSIST";
const ordinary = Buffer.from([79, 82, 68, 0, 255, 128, 195, 169, 10]);
const output = Buffer.from("stdout unchanged\n");
const fixtureSource = String.raw`
import fs from 'node:fs';import path from 'node:path';
const mode=process.argv[2],ready=process.argv[3],release=process.argv[4];
const event={version:1,event:'browser-startup-relaunch',surface:'playwright',engine:'chromium',attempt:2,maxAttempts:2,timeoutMs:15000,pid:process.pid};
fs.appendFileSync(process.env.RICHVIEW_BROWSER_EVENT_LOG,JSON.stringify(event)+'\n');
process.stdout.write('stdout unchanged\n');
process.stderr.write(Buffer.from([79,82,68,0,255,128,195,169,10]));
process.stderr.write('RICHVIEW_BROWSER_RELAUNCH '+JSON.stringify(event)+'\n');
const packet=JSON.stringify({id:1,method:'Runtime.evaluate',params:{executionContextId:'context-one',expression:'PRIVATE_PROTOCOL_SOURCE_NEVER_PERSIST'}});
process.stderr.write('pw:prot');process.stderr.write('ocol SEND '+String.fromCharCode(9658)+' '+packet+'\n');
if(mode==='cap')for(let i=0;i<18000;i++)process.stderr.write('pw:protocol '+String.fromCharCode(9664)+' RECV '+JSON.stringify({method:'Page.frameAttached',params:{frameId:'f'.repeat(160)+i,parentFrameId:'p'.repeat(160),source:'PRIVATE_PROTOCOL_SOURCE_NEVER_PERSIST'}})+'\n');
if(mode==='split') {
  const first='pw:protocol SEND '+String.fromCharCode(9658)+' '+JSON.stringify({id:2,method:'Runtime.callFunction',params:{functionDeclaration:'PRIVATE_PROTOCOL_SOURCE_NEVER_PERSIST'+'x'.repeat(9*1024*1024)}}).slice(0,-2);
  await new Promise(resolve=>process.stderr.write(first,resolve));
  fs.writeFileSync(ready,JSON.stringify({pid:process.pid}));
  if(!fs.existsSync(release))await new Promise(resolve=>{const w=fs.watch(path.dirname(release),()=>{if(fs.existsSync(release)){w.close();resolve();}});if(fs.existsSync(release)){w.close();resolve();}});
  process.stderr.write('}}\n');
}
process.stderr.write('ordinary tail without newline');
if(mode==='abort'||mode==='timeout'){fs.writeFileSync(ready,JSON.stringify({pid:process.pid}));setInterval(()=>{},1000);}
else process.exitCode=mode==='nonzero'?7:0;
`;
function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), "rv-proc-firefox-"));
  mkdirSync(join(dir, "scripts"));
  writeFileSync(join(dir, "scripts/verify-browser.mjs"), fixtureSource);
  writeFileSync(join(dir, "scripts/other.mjs"), fixtureSource);
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const execute = (mode, extra = {}) => {
    const logPath = join(dir, `${mode}-${Math.random().toString(16).slice(2)}.log`), protocolPath = extra.protocolPath ?? logPath + ".protocol.jsonl";
    const ready = logPath + ".ready", release = logPath + ".release";
    const env = gateEnv({ RICHVIEW_VERIFY_BROWSER: "firefox", RV_GATE_FIREFOX_PROTOCOL_LOG: protocolPath, DEBUG: "pw:protocol", DEBUG_COLORS: "0", ...extra.env });
    const promise = runLogged("node", [extra.script ?? "scripts/verify-browser.mjs", mode, ready, release], { cwd: dir, env, logPath, timeoutMs: extra.timeoutMs ?? 30000, signal: extra.signal, onOutput: extra.onOutput });
    return { promise, logPath, protocolPath, ready, release };
  };
  const until = check => new Promise((accept, reject) => {
    const timer = setTimeout(() => { watcher.close(); reject(new Error("Fixture acknowledgement did not arrive")); }, 15000);
    const done = () => { try { if (check()) { clearTimeout(timer); watcher.close(); accept(); } } catch (error) { clearTimeout(timer); watcher.close(); reject(error); } };
    const watcher = watch(dir, done); t.after(() => { clearTimeout(timer); watcher.close(); }); done();
  });
  return { dir, execute, until };
}
function bodyWithoutStdout(logPath) {
  const data = readFileSync(logPath), start = data.indexOf(10) + 1, end = data.lastIndexOf(Buffer.from("\nrv-gate: exit "));
  let body = data.subarray(start, end), index = body.indexOf(output);
  assert.ok(index >= 0, "stdout is unchanged");
  body = Buffer.concat([body.subarray(0, index), body.subarray(index + output.length)]);
  return body;
}
function keptOrdinary(result) {
  const event = { ...result.browserRelaunches[0] }; delete event.journal;
  return Buffer.concat([ordinary, Buffer.from("RICHVIEW_BROWSER_RELAUNCH " + JSON.stringify(event) + "\nordinary tail without newline")]);
}

test("the marker is inherited only through an explicit command environment", async t => {
  const f = fixture(t), marker = join(f.dir, "ambient-sidecar"), old = process.env.RV_GATE_FIREFOX_PROTOCOL_LOG, browser = process.env.RICHVIEW_VERIFY_BROWSER;
  t.after(() => {
    if (old === undefined) delete process.env.RV_GATE_FIREFOX_PROTOCOL_LOG; else process.env.RV_GATE_FIREFOX_PROTOCOL_LOG = old;
    if (browser === undefined) delete process.env.RICHVIEW_VERIFY_BROWSER; else process.env.RICHVIEW_VERIFY_BROWSER = browser;
  });
  process.env.RV_GATE_FIREFOX_PROTOCOL_LOG = marker; process.env.RICHVIEW_VERIFY_BROWSER = "firefox";
  const logPath = join(f.dir, "ambient.log");
  const result = await runLogged("node", ["scripts/verify-browser.mjs", "normal"], { cwd: f.dir, logPath });
  assert.equal(result.status, "pass"); assert.equal(result.firefoxProtocol, undefined);
  assert.equal(existsSync(marker), false);
  assert.equal(readFileSync(logPath).includes(Buffer.from(PRIVATE)), true, "An ambient marker cannot filter the real child's ordinary path");
  assert.equal("RV_GATE_FIREFOX_PROTOCOL_LOG" in gateEnv(), false);
  assert.equal(gateEnv({ RV_GATE_FIREFOX_PROTOCOL_LOG: "/explicit" }).RV_GATE_FIREFOX_PROTOCOL_LOG, "/explicit");
});

test("a marked canonical real child retains exact ordinary bytes, PID and journal for pass and nonzero", async t => {
  const f = fixture(t);
  for (const mode of ["normal", "nonzero"]) {
    const forwarded = [], run = f.execute(mode, { onOutput: chunk => forwarded.push(Buffer.from(chunk)) }), result = await run.promise;
    assert.equal(result.exitCode, mode === "normal" ? 0 : 7);
    assert.equal(result.status, mode === "normal" ? "pass" : "fail");
    assert.equal(result.browserRelaunches.length, 1);
    assert.deepEqual(bodyWithoutStdout(run.logPath), keptOrdinary(result));
    const visible = Buffer.concat(forwarded), stdoutAt = visible.indexOf(output);
    assert.ok(stdoutAt >= 0); assert.deepEqual(Buffer.concat([visible.subarray(0, stdoutAt), visible.subarray(stdoutAt + output.length)]), keptOrdinary(result));
    assert.equal(visible.includes(Buffer.from(PRIVATE)), false, "Live forwarding must preserve the same source-safe ordinary bytes as the log");
    assert.equal(readFileSync(run.logPath).includes(Buffer.from(PRIVATE)), false);
    assert.equal(readFileSync(run.protocolPath, "utf8").includes(PRIVATE), false);
    assert.equal(result.firefoxProtocol.ended, true); assert.equal(result.firefoxProtocol.critical_complete, true);
    assert.ok(result.browserRelaunches[0].pid > 0);
    assert.deepEqual(JSON.parse(readFileSync(run.protocolPath + ".meta.json")), result.firefoxProtocol);
  }
});

test("marker scope leaves other scripts and engines byte-transparent", async t => {
  const f = fixture(t);
  for (const extra of [{ script: "scripts/other.mjs" }, { env: { RICHVIEW_VERIFY_BROWSER: "chromium" } }, { env: { RV_GATE_FIREFOX_PROTOCOL_LOG: "" } }]) {
    const run = f.execute("normal", extra), result = await run.promise;
    assert.equal(result.status, "pass"); assert.equal(result.firefoxProtocol, undefined);
    assert.equal(existsSync(run.protocolPath), false);
    assert.equal(readFileSync(run.logPath).includes(Buffer.from(PRIVATE)), true, "Unmarked command output retains its original bytes");
  }
});

test("real stderr cap and sidecar write loss stay incomplete without changing a passing command", async t => {
  const f = fixture(t), badPath = join(f.dir, "sidecar-directory"); mkdirSync(badPath);
  for (const [mode, extra, field] of [["cap", {}, "capReached"], ["normal", { protocolPath: badPath }, "writeLost"], ["normal", { protocolPath: join(f.dir, "metadata-collision") }, "metadataWriteLost"]]) {
    if (field === "metadataWriteLost") mkdirSync(extra.protocolPath + ".meta.json");
    const run = f.execute(mode, extra), result = await run.promise;
    assert.equal(result.status, "pass"); assert.equal(result.exitCode, 0);
    assert.equal(result.firefoxProtocol[field], true); assert.equal(result.firefoxProtocol.critical_complete, false);
    assert.ok(result.firefoxProtocol.bytes <= 4 * 1024 * 1024);
    const log = readFileSync(run.logPath); assert.equal(log.includes(Buffer.from(PRIVATE)), false);
    assert.ok(log.includes(ordinary)); assert.match(log.toString("utf8"), /Firefox protocol diagnostics incomplete/);
    assert.equal(result.browserRelaunches.length, 1);
  }
});

test("a real split overlong protocol record is consumed before its continuation and ordinary EOF is flushed", async t => {
  const f = fixture(t), run = f.execute("split");
  await f.until(() => existsSync(run.ready) && existsSync(run.protocolPath) && readFileSync(run.protocolPath, "utf8").includes('"truncated":true'));
  writeFileSync(run.release, "continue");
  const result = await run.promise;
  assert.equal(result.status, "pass"); assert.equal(result.firefoxProtocol.lineLimit, true);
  assert.equal(result.firefoxProtocol.critical_complete, false);
  const log = readFileSync(run.logPath); assert.equal(log.includes(Buffer.from(PRIVATE)), false);
  assert.ok(log.includes(ordinary)); assert.match(log.toString("utf8"), /ordinary tail without newline/);
});

test("abort and the existing timeout retain their failure and journal while finalizing diagnostics", { timeout: 20000 }, async t => {
  const f = fixture(t);
  for (const mode of ["abort", "timeout"]) {
    const controller = new AbortController(), run = f.execute(mode, { signal: controller.signal, timeoutMs: 5000 });
    await f.until(() => existsSync(run.ready) && existsSync(run.protocolPath) && readFileSync(run.protocolPath, "utf8").includes('"method":"Runtime.evaluate"'));
    if (mode === "abort") controller.abort();
    const result = await run.promise;
    assert.equal(result.status, "fail"); assert.equal(result.exitCode, mode === "abort" ? 127 : 1);
    assert.equal(result.browserRelaunches.length, 1); assert.equal(result.firefoxProtocol.ended, true);
    assert.equal(readFileSync(run.logPath).includes(Buffer.from(PRIVATE)), false);
    if (mode === "timeout") assert.match(readFileSync(run.logPath, "utf8"), /timed out after 5000 ms/);
  }
});
