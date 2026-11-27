// Host reason (ARC-03): command completion must include the complete flushed evidence log.
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import fs, { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { syncBuiltinESMExports } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Writable } from "node:stream";
import test from "node:test";
import { runLogged } from "../lib/proc.mts";

test("a completed child cannot resolve before its log finishes flushing", async t => {
	const chunks: string[] = [];
	let release: (() => void) | undefined;
	const {promise: finalizing, resolve: reached}=Promise.withResolvers<void>();
	const sink = new Writable({
		write(chunk, _encoding, done) { chunks.push(chunk.toString()); done(); },
		final(done) { release = () => { release = undefined; done(); }; reached(); },
	});
	const replacement = t.mock.method(fs, "createWriteStream", () => sink);
	syncBuiltinESMExports();
	t.after(() => { replacement.mock.restore(); syncBuiltinESMExports(); });
	let resolved = false;
	const running = runLogged(process.execPath, ["-e", 'console.log(JSON.stringify({done:true}))'], { logPath: "controlled-log" }).then(result => { resolved = true; return result; });
	try {
		await finalizing;
		// Drain a promise microtask, with the stream's _final still explicitly blocked.
		await Promise.resolve();
		assert.equal(resolved, false, "child close is not a flushed command log");
		assert.ok(release);
		release();
		assert.equal((await running).status, "pass");
		assert.equal(sink.writableFinished, true);
		assert.match(chunks.join(""), /\n\{"done":true\}\n/);
		assert.match(chunks.join(""), /rv-gate: exit 0 after \d+ ms\n$/);
	} finally { release?.(); await running; }
});

test("concurrent real child logs contain all output immediately after command completion", async t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-log-flush-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const size = 256 * 1024;
	await Promise.all(Array.from({ length: 4 }, async (_, worker) => {
		const stream = worker % 2 ? "stderr" : "stdout";
		const code = `process.${stream}.write('A'.repeat(${size})+'\\n'+JSON.stringify({done:true})+'\\n');`;
		for (let round = 0; round < 4; round++) {
			const logPath = join(dir, `${worker}-${round}.log`);
			assert.equal((await runLogged(process.execPath, ["-e", code], { logPath })).status, "pass");
			const full = readFileSync(logPath, "utf8"), body = full.slice(full.indexOf("\n") + 1);
			assert.equal((body.match(/A/g) ?? []).length, size);
			assert.match(body, /\n\{"done":true\}\n/);
			assert.match(body, /rv-gate: exit 0 after \d+ ms\n$/);
		}
	}));
});

test("an errored command log still settles and cannot report a passing command", { timeout: 5000 }, async t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-log-error-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const result = await runLogged(process.execPath, ["-e", "console.log('done')"], { logPath: join(dir, "missing", "step.log") });
	assert.equal(result.status, "fail");
	assert.match(result.reason ?? "", /Could not finish command log/);
});

test("abort drains a command's grandchild before releasing its caller", {timeout: 15000}, async t => {
 const dir=mkdtempSync(join(process.cwd(),'tmp/rv-process-group-'));
 t.after(()=>rmSync(dir,{recursive:true,force:true}));
 const marker=join(dir,'pid'), controller=new AbortController();
 const grandchild=`const fs=require('node:fs');fs.writeFileSync(process.argv[1],String(process.pid));process.on('SIGTERM',()=>setTimeout(()=>process.exit(0),150));setInterval(()=>{},1000);`;
 const parent=`const {spawn}=require('node:child_process');const fs=require('node:fs');fs.writeFileSync(${JSON.stringify(marker+'-group')},String(process.pid));spawn(process.execPath,['-e',${JSON.stringify(grandchild)},${JSON.stringify(marker)}],{stdio:'ignore'});setInterval(()=>{if(process.ppid===1)process.kill(-process.pid,'SIGTERM')},100);`;
 const running=runLogged(process.execPath,['-e',parent],{cwd:dir,logPath:join(dir,'out.log'),signal:controller.signal});
 let pid: number | undefined;
 try {
  const end=Date.now()+5000;
  while(Date.now()<end) {try {pid=Number(readFileSync(marker,'utf8'));break;} catch {await new Promise(r=>setTimeout(r,10));}}
  assert.ok(pid,'grandchild started'); const grandchildPid = pid; controller.abort();
  assert.equal((await running).status,'fail');
  assert.throws(()=>process.kill(grandchildPid,0),{code:'ESRCH'},'grandchild must have exited before the caller resumes');
 } finally {controller.abort(); try {const group=Number(readFileSync(marker+'-group','utf8'));process.kill(-group,'SIGTERM');} catch(error) {if(!['ENOENT','ESRCH'].includes((error as NodeJS.ErrnoException).code ?? ''))throw error;} if(pid) {try {process.kill(pid,'SIGKILL');} catch(error) {if((error as NodeJS.ErrnoException).code!=='ESRCH')throw error;}} await running;}
});


test('a terminal signal drains concurrent owned groups before the parent preserves its signal exit', {timeout: 15000}, async t => {
 const dir=mkdtempSync(join(process.cwd(),'tmp/rv-signal-groups-')); t.after(()=>rmSync(dir,{recursive:true,force:true}));
 const module=fileURLToPath(new URL('../lib/proc.mts',import.meta.url)), driver=join(dir,'driver.mjs'), completed=join(dir,'completed');
 const grand=`const fs=require('node:fs');fs.writeFileSync(process.argv[1],String(process.pid));process.on('SIGTERM',()=>setTimeout(()=>process.exit(0),150));setInterval(()=>{},1000);`;
 const child=`const {spawn}=require('node:child_process');require('node:fs').writeFileSync(process.argv[2]+'-group',String(process.pid));spawn(process.execPath,['-e',process.argv[1],process.argv[2]],{stdio:'ignore'});setInterval(()=>{},1000);`;
 writeFileSync(driver,`import {runLogged} from ${JSON.stringify(module)};import {writeFileSync} from 'node:fs';\nconst kill=process.kill.bind(process);process.kill=(pid,signal)=>{if(pid<0&&signal===0){const error=new Error('simulated zombie-only process group');error.code='EPERM';throw error}return kill(pid,signal)};\nsetInterval(()=>{if(process.ppid===1)process.kill(process.pid,'SIGTERM')},100).unref();\ntry {await Promise.all([0,1].map(i=>runLogged(process.execPath,['-e',${JSON.stringify(child)},${JSON.stringify(grand)},${JSON.stringify(dir)}+'/pid-'+i],{logPath:${JSON.stringify(dir)}+'/log-'+i})));} finally {writeFileSync(${JSON.stringify(completed)},'released');}\n`);
 const parent=spawn(process.execPath,[driver],{stdio:['ignore','ignore','pipe']}), closed=new Promise(r=>parent.once('close',(code,signal)=>r({code,signal}))); const pids=[]; let stderr='';parent.stderr.on('data',chunk=>{stderr+=chunk});
 try {
  const end=Date.now()+5000; while(Date.now()<end && pids.filter(Boolean).length<2) {for(let i=0;i<2;i++)if(!pids[i])try{pids[i]=Number(readFileSync(join(dir,'pid-'+i),'utf8'));}catch{};if(pids.filter(Boolean).length===2)break;await new Promise(r=>setTimeout(r,10));}
  assert.equal(pids.filter(Boolean).length,2); parent.kill('SIGTERM'); const result=await closed; assert.deepEqual(result,{code:null,signal:'SIGTERM'},stderr);
  assert.equal(readFileSync(completed,'utf8'),'released'); for(const pid of pids) assert.throws(()=>process.kill(pid,0),{code:'ESRCH'});
 } finally {if(parent.exitCode===null&&parent.signalCode===null)parent.kill('SIGTERM');for(let i=0;i<2;i++)try{const group=Number(readFileSync(join(dir,'pid-'+i+'-group'),'utf8'));process.kill(-group,'SIGTERM');}catch(error){if(!['ENOENT','ESRCH'].includes((error as NodeJS.ErrnoException).code ?? ''))throw error;}for(const pid of pids.filter(Boolean))try{process.kill(pid,'SIGKILL');}catch(error){if((error as NodeJS.ErrnoException).code!=='ESRCH')throw error;}await closed;}
});

test('an existing gate signal handler remains in charge of cleanup and exit', {timeout: 10000}, async t => {
 const dir=mkdtempSync(join(process.cwd(),'tmp/rv-signal-handler-'));t.after(()=>rmSync(dir,{recursive:true,force:true}));
 const module=fileURLToPath(new URL('../lib/proc.mts',import.meta.url)),driver=join(dir,'driver.mjs'),marker=join(dir,'ready'),out=join(dir,'cleanup');
 const child=`require('node:fs').writeFileSync(process.argv[1],String(process.pid));setInterval(()=>{},1000);`;
 writeFileSync(driver,`import {runLogged} from ${JSON.stringify(module)};import {writeFileSync} from 'node:fs';setInterval(()=>{if(process.ppid===1)process.kill(process.pid,'SIGTERM')},100).unref();let handled=false;process.once('SIGTERM',()=>{handled=true});const result=await runLogged(process.execPath,['-e',${JSON.stringify(child)},${JSON.stringify(marker)}],{logPath:${JSON.stringify(join(dir,'log'))}});writeFileSync(${JSON.stringify(out)},JSON.stringify({handled,status:result.status}));`);
 const parent=spawn(process.execPath,[driver],{stdio:'ignore'}),closed=new Promise(r=>parent.once('close',(code,signal)=>r({code,signal})));
 try {
  const end=Date.now()+5000;while(Date.now()<end&&!existsSync(marker)) await new Promise(r=>setTimeout(r,10));assert.ok(existsSync(marker));
  parent.kill('SIGTERM');assert.deepEqual(await closed,{code:0,signal:null});assert.deepEqual(JSON.parse(readFileSync(out,'utf8')),{handled:true,status:'fail'});
 } finally {if(parent.exitCode===null&&parent.signalCode===null)parent.kill('SIGTERM');try{const group=Number(readFileSync(marker,'utf8'));process.kill(-group,'SIGTERM');}catch(error){if(!['ENOENT','ESRCH'].includes((error as NodeJS.ErrnoException).code ?? ''))throw error;}await closed;}
});

test('an uninspectable process group fails after the bounded drain instead of throwing or hanging', {timeout: 10000}, t => {
 const dir=mkdtempSync(join(process.cwd(),'tmp/rv-group-unknown-'));t.after(()=>rmSync(dir,{recursive:true,force:true}));
 const module=fileURLToPath(new URL('../lib/proc.mts',import.meta.url)),driver=join(dir,'driver.mjs'),log=join(dir,'log'),out=join(dir,'result');
 writeFileSync(driver,`import cp from 'node:child_process';import{syncBuiltinESMExports}from'node:module';import{writeFileSync}from'node:fs';import{runLogged}from${JSON.stringify(module)};
const original=cp.spawnSync;cp.spawnSync=(cmd,...args)=>cmd==='ps'?{status:1,stdout:''}:original(cmd,...args);syncBuiltinESMExports();
let now=Date.now();Date.now=()=>now;const controller=new AbortController();let fired=false;
const result=await runLogged(process.execPath,['-e','console.log("ready");setInterval(()=>{},1000)'],{logPath:${JSON.stringify(log)},signal:controller.signal,onOutput:()=>{if(!fired){fired=true;controller.abort();now+=8100;}}});
writeFileSync(${JSON.stringify(out)},JSON.stringify(result));`);
 const driverResult=spawnSync(process.execPath,[driver],{encoding:'utf8',timeout:8000,killSignal:'SIGKILL'});assert.equal(driverResult.status,0,driverResult.stderr);
 assert.equal(JSON.parse(readFileSync(out,'utf8')).status,'fail');assert.match(readFileSync(log,'utf8'),/command group did not drain or could not be inspected after SIGKILL/);
});

test('a zombie-only group counts as drained', {timeout: 10000}, t => {
 const dir=mkdtempSync(join(process.cwd(),'tmp/rv-group-zombie-'));t.after(()=>rmSync(dir,{recursive:true,force:true}));
 const module=fileURLToPath(new URL('../lib/proc.mts',import.meta.url)),driver=join(dir,'driver.mjs'),log=join(dir,'log'),out=join(dir,'result');
 writeFileSync(driver,`import cp from 'node:child_process';import{syncBuiltinESMExports}from'node:module';import{writeFileSync}from'node:fs';import{runLogged}from${JSON.stringify(module)};
let childPid;const original=cp.spawnSync;cp.spawnSync=(cmd,...args)=>cmd==='ps'?{status:0,stdout:'1 1 S\\n'+childPid+' '+childPid+' Z+\\n'}:original(cmd,...args);syncBuiltinESMExports();
const controller=new AbortController();let fired=false;const result=await runLogged(process.execPath,['-e','console.log(process.pid);setInterval(()=>{},1000)'],{logPath:${JSON.stringify(log)},signal:controller.signal,onOutput:chunk=>{if(!fired){fired=true;childPid=Number(String(chunk).trim());controller.abort();}}});
writeFileSync(${JSON.stringify(out)},JSON.stringify(result));`);
 const result=spawnSync(process.execPath,[driver],{encoding:'utf8',timeout:8000,killSignal:'SIGKILL'});assert.equal(result.status,0,result.stderr);
 assert.equal(JSON.parse(readFileSync(out,'utf8')).status,'fail');assert.doesNotMatch(readFileSync(log,'utf8'),/did not drain/);
});

test('the drain deadline also bounds a child that never closes its pipes', {timeout: 10000}, t => {
 const dir=mkdtempSync(join(process.cwd(),'tmp/rv-group-open-pipes-'));t.after(()=>rmSync(dir,{recursive:true,force:true}));
 const module=fileURLToPath(new URL('../lib/proc.mts',import.meta.url)),driver=join(dir,'driver.mjs'),log=join(dir,'log'),out=join(dir,'result'),pidFile=join(dir,'pid');
 writeFileSync(driver,`import{writeFileSync}from'node:fs';import{runLogged}from${JSON.stringify(module)};
const kill=process.kill.bind(process);process.kill=(pid,signal)=>pid<0?true:kill(pid,signal);
const later=setTimeout;globalThis.setTimeout=(fn,ms,...args)=>later(fn,ms===8000?50:ms===5000?25:ms,...args);
const controller=new AbortController();let fired=false;const result=await runLogged(process.execPath,['-e','console.log(process.pid);setInterval(()=>{},1000)'],{logPath:${JSON.stringify(log)},signal:controller.signal,onOutput:chunk=>{if(!fired){fired=true;writeFileSync(${JSON.stringify(pidFile)},String(chunk).trim());controller.abort();}}});
writeFileSync(${JSON.stringify(out)},JSON.stringify(result));process.exit(0);`);
 try {
  const result=spawnSync(process.execPath,[driver],{encoding:'utf8',timeout:3000,killSignal:'SIGKILL'});assert.equal(result.status,0,result.stderr);
  assert.equal(JSON.parse(readFileSync(out,'utf8')).status,'fail');assert.match(readFileSync(log,'utf8'),/command group did not drain or could not be inspected after SIGKILL/);
 } finally {try{const pid=Number(readFileSync(pidFile,'utf8'));process.kill(pid,'SIGKILL');}catch(error){if(!['ENOENT','ESRCH'].includes((error as NodeJS.ErrnoException).code ?? ''))throw error;}}
});
