// Host reason (ARC-03): exercise startup retries against Playwright and real isolated process trees, without a browser installation.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import test from 'node:test';

type Journal = boolean | string;
type LaunchOptions = {mode?: string; journal?: Journal; quiet?: boolean; outsider?: boolean; foreign?: boolean; cached?: boolean; blockFirstSetup?: boolean; groupReports?: boolean | string; direct?: string};
type TraceRow = {event: string; pid: number; liveGroups?: number[]; timeout?: number; ordinal?: number; pidFileExists?: boolean; quietHeld?: boolean; at: number; timeoutError?: boolean; group?: number; name?: string};
type AttemptRow = {overlap: number[]; pid: number; descendant: number; attempt?: number; quiet?: boolean; source?: string};
type DriverResult = {ok: boolean; name: string; message: string; releasedAt: number; permissionChecks: number; zombiePid: number; [key: string]: unknown};

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const helper = new URL('../lib/browser-launch.mts', import.meta.url).href;
const playwright = new URL('../../../node_modules/playwright/index.mjs', import.meta.url).href;
const shellQuote = (value: string) => "'" + value.replaceAll("'", "'\\''") + "'";

function run(t: test.TestContext, { mode = 'stall', journal = true, quiet = false, outsider = false, foreign = false, cached = false, blockFirstSetup = false, groupReports = false, direct = '' }: LaunchOptions = {}) {
  mkdirSync(join(root, 'tmp'), { recursive: true });
  const dir = mkdtempSync(join(root, 'tmp/browser-startup-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  if (foreign) for (const pkg of ['playwright', 'playwright-core']) cpSync(join(root, 'node_modules', pkg), join(dir, 'node_modules', pkg), { recursive: true });
  let selectedHelper = helper;
  if (cached) {
    mkdirSync(join(dir, 'cached')); mkdirSync(join(dir, 'node_modules/playwright'), { recursive: true });
    cpSync(fileURLToPath(helper), join(dir, 'cached/browser-launch.mts'));
    cpSync(join(root, 'tools/rv-gate/lib/quiet.mts'), join(dir, 'cached/quiet.mts'));
    // Block ancestor SDK resolution, as in the cached gate before the checkout installs packages.
    writeFileSync(join(dir, 'node_modules/playwright/package.json'), JSON.stringify({ name: 'playwright', exports: './not-installed.mjs' }));
    selectedHelper = pathToFileURL(join(dir, 'cached/browser-launch.mts')).href;
  }
  const executable = join(dir, 'browser.sh'), records = join(dir, 'attempts.jsonl'), events = join(dir, 'events.jsonl');
  const launchedExecutable = mode === 'zombie' ? '/usr/bin/true' : executable;
  if (blockFirstSetup) assert.equal(spawnSync('mkfifo', [join(dir, 'entry-gate')]).status, 0);
  // A shell and sleep expose the process-tree failure without competing Node startup time.
  writeFileSync(executable, `#!/bin/sh
records=${shellQuote(records)}
pids=${shellQuote(join(dir, 'pids'))}
attempt="$RICHVIEW_TEST_LAUNCH_ORDINAL"
case "$attempt" in 1|2) ;; *) echo INVALID_FIXTURE_ORDINAL >&2; exit 91 ;; esac
overlap=''
comma=''
if [ -f "$pids" ]; then
  for pid in $(cat "$pids"); do
    if kill -0 "$pid" 2>/dev/null; then overlap="$overlap$comma$pid"; comma=','; fi
  done
fi
quiet=false
if [ -f ${shellQuote(join(dir, '.rv-perf-quiet'))} ]; then quiet=true; fi
if [ "$attempt" = 2 ] && [ ${shellQuote(mode)} = exit ]; then
  printf '{"attempt":2,"pid":%s,"overlap":[%s],"quiet":%s}\n' "$$" "$overlap" "$quiet" >> "$records"
  echo SECOND_STARTUP_FAILURE >&2
  exit 7
fi
if [ "$attempt" = 1 ] && [ ${shellQuote(String(blockFirstSetup))} = true ]; then read entry < ${shellQuote(join(dir, 'entry-gate'))}; fi
if [ ${shellQuote(mode)} = orphan ]; then
  sleep 300 </dev/null >/dev/null 2>&1 3>&- 4>&- &
else
  sleep 300 &
fi
descendant=$!
printf '%s %s\n' "$$" "$descendant" >> ${shellQuote(join(dir, 'observed-pids'))}
printf '%s %s\n' "$$" "$descendant" > "$pids"
printf '{"attempt":%s,"pid":%s,"descendant":%s,"overlap":[%s],"quiet":%s}\n' "$attempt" "$$" "$descendant" "$overlap" "$quiet" >> "$records"
if [ ${shellQuote(mode)} = orphan ]; then exit 7; fi
wait
`);
  chmodSync(executable, 0o755);
  const driver = `
import { chromium, errors } from ${JSON.stringify(playwright)};
import { launchBrowser } from ${JSON.stringify(selectedHelper)};
import { existsSync, readFileSync, writeFileSync, unlinkSync } from 'node:fs';
import { channel } from 'node:diagnostics_channel';
const result={}, trace=[]; let lastLaunchError, rootReaped=false;
${mode === 'terminal' ? `const terminalFailure=Error('SECOND_STARTUP_FAILURE');let failSecondLaunch,pendingTerminalLaunch;` : ''}
const systemKill=process.kill;
${groupReports ? `// Fault only the reported group state; the following ESRCH must come from the real OS.
process.kill=function(pid,signal){
  if(rootReaped&&pid===-result.zombiePid&&signal===0){
    result.permissionChecks=(result.permissionChecks||0)+1;
    if(${JSON.stringify(groupReports)}==='persistent'||result.permissionChecks===1)throw Object.assign(Error('controlled group state denied'),{code:'EPERM'});
    try{return systemKill.call(process,pid,signal);}catch(error){if(error.code==='ESRCH')result.verifiedAbsent=true;throw error;}
  }
  return systemKill.call(process,pid,signal);
};` : ''}
const spawning=channel('tracing:child_process.spawn:start');
const observe=({process:child,options})=>{
  if(options.file!==${JSON.stringify(launchedExecutable)})return;
  trace.push({event:'spawn-start',at:Date.now()});
  child.once('spawn',()=>{
    trace.push({event:'spawn',pid:child.pid,at:Date.now()});
    ${quiet ? "if(trace.filter(item=>item.event==='spawn').length===1)occupyQuiet();" : ''}
    ${mode === 'terminal' ? `if(trace.filter(item=>item.event==='spawn').length===2){
      const overlap=trace.filter(item=>item.event==='spawn'&&item.pid!==child.pid).flatMap(item=>{
        try{process.kill(-item.pid,0);return [item.pid];}catch(error){if(error.code==='ESRCH')return [];throw error;}
      });
      const quiet=existsSync(${JSON.stringify(join(dir, '.rv-perf-quiet'))});
      // This is a parent observation of the real SDK spawn, not evidence of child execution.
      writeFileSync(${JSON.stringify(records)},JSON.stringify({attempt:2,pid:child.pid,overlap,quiet,source:'parent-spawn'})+'\\n',{flag:'a'});
      result.terminalSpawnPid=child.pid;
      failSecondLaunch(terminalFailure);
    }` : ''}
  });
  child.once('error',error=>trace.push({event:'spawn-error',message:error.message,at:Date.now()}));
  child.once('exit',(code,signal)=>{if(child.pid===result.zombiePid)rootReaped=true;trace.push({event:'exit',pid:child.pid,code,signal,at:Date.now()});});
  child.once('close',(code,signal)=>trace.push({event:'close',pid:child.pid,code,signal,at:Date.now()}));
};
spawning.subscribe(observe);
${foreign ? `const sdk=await import(${JSON.stringify(pathToFileURL(join(dir, 'node_modules/playwright/index.mjs')).href)});if(sdk.errors.TimeoutError===errors.TimeoutError)throw Error('second SDK shares the class identity');result.distinctTimeoutClass=true;` : ''}
${quiet || outsider || mode === 'orphan' || mode === 'zombie' ? `const {spawn}=await import('node:child_process');` : ''}
${outsider ? `const outsideReady=new Promise(resolve=>setTimeout(()=>{
  const child=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{detached:true,stdio:['ignore','pipe','pipe','pipe','pipe']});
  const done=new Promise(accept=>child.once('close',accept));
  child.once('spawn',()=>resolve({child,done}));
},100));` : ''}
${quiet ? `const occupyQuiet=()=>{
  const owner=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore'});
  writeFileSync(${JSON.stringify(join(dir, '.rv-perf-quiet'))},String(owner.pid));
  setTimeout(()=>{result.releasedAt=Date.now();unlinkSync(${JSON.stringify(join(dir, '.rv-perf-quiet'))});owner.kill('SIGKILL');},1400);
};` : ''}
${mode === 'orphan' ? `// Isolate the closed-root condition from Playwright's independent cleanup, which can mask it.
const closedLeader={name:()=>chromium.name(),launch:options=>new Promise((_,reject)=>{
  const child=spawn(options.executablePath,[],{detached:true,env:options.env,stdio:['ignore','pipe','pipe','pipe','pipe']});
  child.once('error',reject);
  child.once('close',code=>{
    result.leaderClosedBeforeReject=true;
    const descendant=Number(readFileSync(${JSON.stringify(join(dir, 'observed-pids'))},'utf8').trim().split(/\\s+/)[1]);
    try{process.kill(descendant,0);result.orphanAliveBeforeReject=true;}catch(error){if(error.code!=='ESRCH')throw error;result.orphanAliveBeforeReject=false;}
    reject(Error('Fixture launcher exited '+code));
  });
})};` : ''}
${mode === 'zombie' ? `const zombieLeader={name:()=>chromium.name(),launch:async options=>{
  const child=spawn(options.executablePath,[],{detached:true,stdio:['ignore','pipe','pipe','pipe','pipe']});
  result.zombiePid=child.pid;
  const wait=new Int32Array(new SharedArrayBuffer(4));
  for(let n=0;n<150;n++){
    Atomics.wait(wait,0,0,5);
    try{process.kill(-child.pid,0);result.unreapedGroup='live';}
    catch(error){result.unreapedGroup=error.code;if(error.code==='EPERM'||error.code==='ESRCH')break;throw error;}
  }
  throw Error('fixture leader exited during startup');
}};` : ''}
const actualBrowserType=${foreign ? 'sdk.chromium' : mode === 'orphan' ? 'closedLeader' : mode === 'zombie' ? 'zombieLeader' : 'chromium'};
const observedBrowserType={name:()=>actualBrowserType.name(),launch:async options=>{
  const ordinal=trace.filter(item=>item.event==='launch-call').length+1;
  const liveGroups=trace.filter(item=>item.event==='spawn').flatMap(item=>{
    try{process.kill(-item.pid,0);return [item.pid];}catch(error){if(error.code==='ESRCH')return [];throw error;}
  });
  trace.push({event:'launch-call',ordinal,timeout:options.timeout,liveGroups,pidFileExists:existsSync(${JSON.stringify(join(dir, 'pids'))}),quietHeld:existsSync(${JSON.stringify(join(dir, '.rv-perf-quiet'))}),at:Date.now()});
  ${mode === 'terminal' ? `const terminal=ordinal===2?new Promise((_,reject)=>{failSecondLaunch=reject;}):null;` : ''}
  try{
    const startup=actualBrowserType.launch({...options,env:{...process.env,...options.env,RICHVIEW_TEST_LAUNCH_ORDINAL:String(ordinal)}});
    ${mode === 'terminal' ? `if(terminal){pendingTerminalLaunch=startup;return await Promise.race([startup,terminal]);}` : ''}
    return await startup;
  }
  catch(error){lastLaunchError=error;trace.push({event:'launch-reject',name:error.name,message:error.message,timeoutError:error instanceof ${foreign ? 'sdk.errors.TimeoutError' : 'errors.TimeoutError'},at:Date.now()});throw error;}
}};
try {
  ${direct || `await launchBrowser(observedBrowserType,{executablePath:${JSON.stringify(launchedExecutable)},headless:true,timeout:750}${foreign ? ',{TimeoutError:sdk.errors.TimeoutError}' : ',{TimeoutError:errors.TimeoutError}'});`}
  result.ok=true;
} catch(error) { result.ok=false;result.name=error.name;result.message=error.message;result.originalLaunchError=error===lastLaunchError;${mode === 'terminal' ? `result.terminalErrorIdentity=error===terminalFailure;
result.groupsAtTerminalReturn=trace.filter(item=>item.event==='spawn').flatMap(item=>{
  try{process.kill(-item.pid,0);return [item.pid];}catch(error){if(error.code==='ESRCH')return [];throw error;}
});` : ''} }
${mode === 'terminal' ? `// The helper already drained its real child; also settle the underlying SDK promise.
// A bad-helper control keeps its observed failure, then cleans up before waiting on the SDK.
for(const pid of result.groupsAtTerminalReturn??[]){
  try{systemKill.call(process,-pid,'SIGKILL');}catch(error){if(error.code!=='ESRCH')throw error;}
}
if(pendingTerminalLaunch){await pendingTerminalLaunch.then(browser=>browser.close(),error=>{
  result.sdkSecondErrorName=error.name;trace.push({event:'sdk-second-settled',name:error.name,at:Date.now()});
});result.sdkSecondSettled=true;}` : ''}
process.kill=systemKill;
${outsider ? `const outside=await outsideReady;
try{process.kill(outside.child.pid,0);result.outsiderSurvived=true;}catch(e){if(e.code!=='ESRCH')throw e;result.outsiderSurvived=false;}
if(result.outsiderSurvived)process.kill(-outside.child.pid,'SIGKILL');
await outside.done;` : ''}
spawning.unsubscribe(observe);
console.log('RESULT '+JSON.stringify({result,trace}));
`;
  const child = spawnSync(process.execPath, ['--input-type=module', '-e', driver], {
    cwd: root, encoding: 'utf8', timeout: 15000,
    env: { ...process.env, RV_SHARED_TARGET: dir, RICHVIEW_BROWSER_EVENT_LOG: journal === true ? events : journal === false ? '' : journal === 'directory' ? dir : journal },
  });
  assert.equal(child.error, undefined, child.stderr);
  assert.equal(child.status, 0, child.stderr);
  const {result,trace} = JSON.parse(child.stdout.split('\n').find(line => line.startsWith('RESULT '))!.slice(7)) as {result: DriverResult; trace: TraceRow[]};
  const lines = <Row,>(path: string): Row[] => { try { return readFileSync(path, 'utf8').trim().split('\n').filter(Boolean).map(line => JSON.parse(line) as Row); } catch (error) { if((error as NodeJS.ErrnoException).code==='ENOENT')return []; throw error; } };
  const attempts = lines<AttemptRow>(records);
  const launches = trace.filter((item: { event: string; }) => item.event === 'launch-call');
  const spawns = trace.filter((item: { event: string; }) => item.event === 'spawn');
  let fixturePids: number[] = [];
  try { fixturePids = readFileSync(join(dir, 'observed-pids'), 'utf8').trim().split(/\s+/).map(Number); }
  catch (error) { if((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
  // Failed regression controls must not leave their deliberately created orphan behind.
  t.after(() => { if (process.platform !== 'win32') for (const item of spawns) { try { process.kill(-item.pid, 'SIGKILL'); } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error; } } });
  return { result, trace, launches, spawns, fixturePids, attempts, events: lines(events), stderr: child.stderr };
}

const posix = { skip: process.platform === 'win32' && 'POSIX process-group fixture' };
test('a real startup timeout drains both owned launch groups and retains exactly one relaunch on terminal timeout', posix, t => {
  const value = run(t, { outsider: true });
  assert.equal(value.result.outsiderSurvived, true, 'an unrelated concurrently spawned process was killed');
  assert.equal(value.result.ok, false);
  assert.equal(value.result.name, 'TimeoutError', JSON.stringify(value));
  assert.equal(value.launches.length, 2, JSON.stringify(value));
  assert.deepEqual(value.launches.map(item => item.liveGroups), [[], []], JSON.stringify(value));
  for (const attempt of value.attempts) assert.deepEqual(attempt.overlap, [], JSON.stringify(value));
  assert.equal(value.spawns.length, 2, JSON.stringify(value));
  for (const item of value.spawns) assert.throws(() => process.kill(-item.pid, 0), { code: 'ESRCH' }, JSON.stringify(value));
  for (const pid of new Set([...value.spawns.map(item => item.pid), ...value.attempts.flatMap(item => [item.pid,item.descendant]), ...value.fixturePids])) {
    assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' }, JSON.stringify(value));
  }
  assert.deepEqual(value.events, [{ version: 1, event: 'browser-startup-relaunch', surface: 'playwright', engine: 'chromium', attempt: 2, maxAttempts: 2, timeoutMs: 750 }]);
  const stderrEvents = value.stderr.split('\n').filter(line => line.startsWith('RICHVIEW_BROWSER_RELAUNCH ')).map(line => JSON.parse(line.slice('RICHVIEW_BROWSER_RELAUNCH '.length)));
  assert.deepEqual(stderrEvents, value.events);
});

test('a second non-timeout startup error is final, and quiet admission precedes the relaunch', posix, t => {
  // A launched root need not execute descendant setup before its startup deadline.
  // The closed-launcher case below owns the nonvacuous live-descendant contract.
  // Deliver the controlled ordinary Error at the real second SDK spawn, without
  // requiring that child to execute its shell error branch inside750ms.
  const value = run(t, { mode: 'terminal', quiet: true, blockFirstSetup: true });
  assert.equal(value.result.ok, false);
  assert.notEqual(value.result.name, 'TimeoutError', JSON.stringify(value));
  assert.match(value.result.message, /SECOND_STARTUP_FAILURE/);
  assert.equal(value.result.originalLaunchError, true, JSON.stringify(value));
  assert.equal(value.result.terminalErrorIdentity, true, JSON.stringify(value));
  assert.deepEqual(value.result.groupsAtTerminalReturn, [], JSON.stringify(value));
  assert.equal(value.result.sdkSecondSettled, true, JSON.stringify(value));
  assert.equal(value.launches.length, 2, JSON.stringify(value));
  assert.equal(value.trace.find((item: { event: string; }) => item.event === 'launch-reject')?.timeoutError, true, JSON.stringify(value));
  assert.deepEqual(value.launches.map(item => item.timeout), [750,750], JSON.stringify(value));
  assert.deepEqual(value.launches.map(item => item.ordinal), [1,2], JSON.stringify(value));
  assert.equal(value.launches[1].pidFileExists, false, JSON.stringify(value));
  assert.deepEqual(value.launches.map(item => item.liveGroups), [[], []], JSON.stringify(value));
  for (const attempt of value.attempts) assert.deepEqual(attempt.overlap, [], JSON.stringify(value));
  assert.ok(value.result.releasedAt > 0);
  assert.equal(value.launches[1].quietHeld, false, JSON.stringify(value));
  assert.ok(value.launches[1].at >= value.result.releasedAt, JSON.stringify(value));
  assert.equal(value.attempts.find(item => item.attempt === 2)?.quiet, false, JSON.stringify(value));
  assert.equal(value.spawns.length, 2, JSON.stringify(value));
  assert.equal(value.result.terminalSpawnPid, value.spawns[1].pid, JSON.stringify(value));
  assert.equal(value.attempts.find(item => item.source === 'parent-spawn')?.pid, value.spawns[1].pid, JSON.stringify(value));
  for (const item of value.spawns) assert.throws(() => process.kill(-item.pid, 0), { code: 'ESRCH' }, JSON.stringify(value));
  for (const pid of value.fixturePids) assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' }, JSON.stringify(value));
  assert.equal(value.events.length, 1);
});

test('a closed launcher still drains its live orphan group without retrying the non-timeout failure', posix, t => {
  const value = run(t, { mode: 'orphan' });
  assert.equal(value.result.ok, false);
  assert.notEqual(value.result.name, 'TimeoutError');
  assert.equal(value.result.leaderClosedBeforeReject, true);
  assert.equal(value.result.orphanAliveBeforeReject, true, JSON.stringify(value));
  assert.equal(value.launches.length, 1, JSON.stringify(value));
  assert.equal(value.spawns.length, 1, JSON.stringify(value));
  assert.equal(value.attempts.length, 1, JSON.stringify(value));
  const { pid, descendant } = value.attempts[0];
  assert.ok(Number.isSafeInteger(descendant) && descendant > 0 && descendant !== pid, JSON.stringify(value));
  assert.deepEqual(value.fixturePids, [pid, descendant], JSON.stringify(value));
  assert.deepEqual(value.events, []);
  assert.throws(() => process.kill(-pid, 0), { code: 'ESRCH' }, JSON.stringify(value));
  for (const observed of [pid, descendant]) assert.throws(() => process.kill(observed, 0), { code: 'ESRCH' }, 'closed leader left a live descendant');
});

test('an unreaped dead leader cannot replace the original startup failure with a group permission error', posix, t => {
  const value = run(t, { mode: 'zombie' });
  if (process.platform === 'darwin') assert.equal(value.result.unreapedGroup, 'EPERM', JSON.stringify(value));
  assert.equal(value.result.originalLaunchError, true, JSON.stringify(value));
  assert.equal(value.result.message, 'fixture leader exited during startup', JSON.stringify(value));
  assert.equal(value.launches.length, 1, JSON.stringify(value));
  assert.deepEqual(value.events, []);
  assert.throws(() => process.kill(-value.result.zombiePid, 0), { code: 'ESRCH' }, JSON.stringify(value));
});

test('permission-only group reports remain present until actual absence is verified or the drain fails', posix, t => {
  const transient = run(t, { mode: 'zombie', groupReports: 'transient' });
  assert.equal(transient.result.originalLaunchError, true, JSON.stringify(transient));
  assert.equal(transient.result.verifiedAbsent, true, JSON.stringify(transient));
  assert.ok(transient.result.permissionChecks > 1, JSON.stringify(transient));
  const persistent = run(t, { mode: 'zombie', groupReports: 'persistent' });
  assert.match(persistent.result.message, /did not drain/, JSON.stringify(persistent));
  assert.equal(persistent.result.verifiedAbsent, undefined, JSON.stringify(persistent));
  assert.equal(persistent.launches.length, 1, JSON.stringify(persistent));
  assert.deepEqual(persistent.events, []);
  assert.throws(() => process.kill(-persistent.result.zombiePid, 0), { code: 'ESRCH' }, JSON.stringify(persistent));
});

test('a genuine timeout from a separate actual Playwright package permits the same bounded recovery', posix, t => {
  const value = run(t, { foreign: true, mode: 'exit' });
  assert.equal(value.result.distinctTimeoutClass, true);
  assert.equal(value.result.ok, false);
  // The genuine foreign timeout is this case's contract. The strict second
  // non-timeout outcome has its own case; a second SDK timeout is also terminal.
  assert.equal(value.trace.find((item: { event: string; }) => item.event === 'launch-reject')?.timeoutError, true, JSON.stringify(value));
  assert.equal(value.result.originalLaunchError, true, JSON.stringify(value));
  assert.equal(value.launches.length, 2, JSON.stringify(value));
  assert.deepEqual(value.launches.map(item => item.liveGroups), [[], []], JSON.stringify(value));
  assert.equal(value.events.length, 1);
});

test('the launcher keeps successful interfaces and non-Chromium options, caps Chromium timeouts, and never retries other errors', t => {
  const value = run(t, { direct: `
const browser={};
for(const [name,timeout,expected] of [['chromium',undefined,15000],['chromium',0,15000],['chromium',60000,15000],['chromium',400,400],['firefox',60000,60000],['webkit',0,0]]){
  const options={timeout,args:['kept']};let calls=0;
  const engine={name:()=>name,launch:async received=>{calls++;if(received.timeout!==expected||received.args!==options.args)throw Error('options changed');if(name!=='chromium'&&received!==options)throw Error('non-Chromium options copied');return browser;}};
  if(await launchBrowser(engine,options,{TimeoutError:errors.TimeoutError})!==browser||calls!==1)throw Error('success interface changed');
}
let calls=0;
try{await launchBrowser({name:()=> 'chromium',launch:async()=>{calls++;const e=Error('not a startup timeout');e.name='TimeoutError';throw e;}},{},{TimeoutError:errors.TimeoutError});}catch(e){if(e.message!=='not a startup timeout')throw e;}
if(calls!==1)throw Error('non-timeout retried');
` });
  assert.equal(value.result.ok, true, value.result.message);
  assert.deepEqual(value.events, []);
});

test('a timeout without an identified launch and an invalid journal both fail before a relaunch', t => {
  const value = run(t, { direct: `let calls=0;try{await launchBrowser({name:()=> 'chromium',launch:async()=>{calls++;throw new errors.TimeoutError('unidentified');}},{},{TimeoutError:errors.TimeoutError});}catch(e){if(!e.message.includes('observed owned launch'))throw e;}if(calls!==1)throw Error('unidentified launch retried');` });
  assert.equal(value.result.ok, true, value.result.message);
  assert.deepEqual(value.events, []);
  const relative = run(t, { journal: 'relative.jsonl', direct: `await launchBrowser({name:()=> 'chromium',launch:async()=>{throw Error('must not launch');}},{},{TimeoutError:errors.TimeoutError});` });
  assert.equal(relative.result.ok, false);
  assert.match(relative.result.message, /absolute path/);
});

test('the cached helper imports without an installed SDK and requires the caller classifier before Chromium launches', t => {
  const value = run(t, { cached: true, direct: `
let calls=0;
const engine={name:()=> 'chromium',launch:async()=>{calls++;throw Error('must not launch');}};
for(const config of [undefined,{}, {TimeoutError:null}, {TimeoutError:Error}]){
  let rejected=false;
  try{await launchBrowser(engine,{},config);}catch(error){if(!error.message.includes('caller SDK public TimeoutError'))throw error;rejected=true;}
  if(!rejected||calls!==0)throw Error('missing classifier did not fail before launch');
}
` });
  assert.equal(value.result.ok, true, value.result.message);
  assert.deepEqual(value.attempts, []);
  assert.deepEqual(value.events, []);
});

test('an unwritable relaunch journal fails visibly after draining and prevents attempt two; standalone still logs', posix, t => {
  const unwritable = run(t, { journal: 'directory' });
  assert.equal(unwritable.result.ok, false);
  assert.match(unwritable.result.message, /EISDIR|illegal operation on a directory/);
  assert.equal(unwritable.launches.length, 1, JSON.stringify(unwritable));
  assert.ok(unwritable.spawns.length > 0, JSON.stringify(unwritable));
  for (const item of unwritable.spawns) assert.throws(() => process.kill(-item.pid, 0), { code: 'ESRCH' }, JSON.stringify(unwritable));
  for (const pid of unwritable.fixturePids) assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' }, JSON.stringify(unwritable));
  // Block before descendant/PID setup; standalone logging depends on actual SDK calls.
  const standalone = run(t, { journal: false, mode: 'exit', blockFirstSetup: true });
  assert.equal(standalone.launches.length, 2, JSON.stringify(standalone));
  assert.equal(standalone.result.ok, false, JSON.stringify(standalone));
  assert.equal(standalone.result.originalLaunchError, true, JSON.stringify(standalone));
  assert.deepEqual(standalone.launches.map(item => item.liveGroups), [[], []], JSON.stringify(standalone));
  assert.equal(standalone.spawns.length, 2, JSON.stringify(standalone));
  for (const item of standalone.spawns) assert.throws(() => process.kill(-item.pid, 0), { code: 'ESRCH' }, JSON.stringify(standalone));
  for (const pid of [...standalone.spawns.map(item => item.pid), ...standalone.fixturePids]) assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' }, JSON.stringify(standalone));
  assert.deepEqual(standalone.events, []);
  assert.equal(standalone.stderr.split('\n').filter(line => line.startsWith('RICHVIEW_BROWSER_RELAUNCH ')).length, 1);
});
