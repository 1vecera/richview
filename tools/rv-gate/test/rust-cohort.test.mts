// Host reason (ARC-03): verify that held Rust shards cover the complete Cargo inventory.
import assert from 'node:assert/strict';
import test from 'node:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import type { GateCommand, GateStep } from "../lib/steps.mts";
import { expandCohort, withCohortHold } from '../lib/rust-tests.mts';

function fixture(t: test.TestContext) {
 mkdirSync(join(process.cwd(), 'tmp'), {recursive:true});
 const root = mkdtempSync(join(process.cwd(), 'tmp/rv-cohort-'));
 t.after(() => rmSync(root, { recursive: true, force: true }));
 const bin = join(root, 'bin'), target = join(root, 'target'), pkg = join(root, 'pkg');
 for (const p of [bin, target, pkg, join(target, 'debug/deps')]) mkdirSync(p, {recursive:true});
 const metadata = {workspace_members:['p'],target_directory:target,packages:[{id:'p',name:'p',version:'1.0.0',manifest_path:join(pkg,'Cargo.toml'),targets:[...['a','b','c'].map(name => ({name,kind:['test'],test:true})),{name:'p',kind:['lib'],test:false,doctest:true}]}]};
 const artifacts = metadata.packages[0].targets.filter(x=>x.test).map(item=>({reason:'compiler-artifact',package_id:'p',profile:{test:true},target:item,executable:join(target,'debug/deps',item.name+'-0123456789abcdef')}));
 for (const row of artifacts) writeFileSync(row.executable, `#!${process.execPath}\nif(process.env.FAIL_BINARY==='${row.target.name}')process.exit(7);console.log('test ${row.target.name} ... ok');\n`, {mode:0o755});
 writeFileSync(join(bin,'cargo'), `#!${process.execPath}\nconst a=process.argv.slice(2);if(a[0]==='metadata')console.log(${JSON.stringify(JSON.stringify(metadata))});else if(a.includes('--no-run')){for(const e of ${JSON.stringify(artifacts)})console.log(JSON.stringify(e));console.log(JSON.stringify({reason:'build-finished',success:true}));}else if(a.includes('--doc')){if(process.env.BAD_DOC)process.exit(2);console.error('   Doc-tests p');}else process.exit(3);\n`, {mode:0o755});
 writeFileSync(join(bin,'rustc'), `#!${process.execPath}\nconsole.log(${JSON.stringify(join(root,'sysroot'))});\n`, {mode:0o755});
 writeFileSync(join(target,'.rv-gate-cargo'), String(process.pid));
 const env: NodeJS.ProcessEnv = {...process.env, PATH:bin+':'+process.env.PATH,CARGO_TARGET_DIR:target,CARGO_HOME:join(root,'cargo-home'),RV_GATE_HOLDER:String(process.pid)};
 for (const key of Object.keys(env)) if (key === 'CARGO_BUILD_TARGET' || /^CARGO_TARGET_.+_RUNNER$/.test(key)) delete env[key];
 return {root,target,env};
}

test('one preparation discovers all test binaries and shards cover each exactly once', async t => {
 const f=fixture(t); const step: GateStep={id:'rust-tests',tier:'full',title:'Rust tests',cards:[],cohort:'rust-debug',run:[{cmd:'cargo',args:['test','--workspace','--locked','--no-fail-fast']}]};
 const expanded=expandCohort(step,{runDir:f.root}); assert.equal(expanded.maxConcurrent,2);
 const run = (c: GateCommand) => spawnSync(c.cmd,c.args,{cwd:f.root,env:f.env,encoding:'utf8'});
 {
  const prep=run(expanded.prepare); assert.equal(prep.status,0,prep.stderr); assert.match(prep.stderr, /Doc-tests p/);
  const results=expanded.shards.map(run); for (const r of results) assert.equal(r.status,0,r.stderr);
  const output=results.map(r=>r.stdout).join('\n'); for (const name of ['a','b','c']) assert.equal((output.match(new RegExp(`test ${name} \\.\\.\\. ok`,'g'))??[]).length,1);
 }
});

test('failed preparation leaves no runnable cohort', async t => {
 const f=fixture(t); const step: GateStep={id:'rust-tests',tier:'full',title:'Rust tests',cards:[],cohort:'rust-debug',run:[{cmd:'cargo',args:['test','--workspace','--locked','--no-fail-fast']}]};
 const expanded=expandCohort(step,{runDir:f.root});
 const shard=spawnSync(expanded.shards[0].cmd,expanded.shards[0].args,{cwd:f.root,env:f.env,encoding:'utf8'});
 assert.notEqual(shard.status,0);
});


test('a failed shard names its binary and can be retried without recompiling', async t => {
 const f=fixture(t); const e=expandCohort({id:'rust-tests',tier:'full',title:'Rust tests',cards:[],cohort:'rust-debug',run:[{cmd:'cargo',args:['test','--workspace','--locked','--no-fail-fast']}]},{runDir:f.root});
 const run=(c: GateCommand,env=f.env)=>spawnSync(c.cmd,c.args,{cwd:f.root,env,encoding:'utf8'});
 assert.equal(run(e.prepare).status,0);
 const failed=e.shards.map(c=>run(c,{...f.env,FAIL_BINARY:'b'}));
 assert.equal(failed.filter(x=>x.status!==0).length,1); assert.match(failed.find(x=>x.status!==0)?.stdout ?? '',/debug\/deps\/b/);
 const retry=run(e.shards[failed.findIndex(x=>x.status!==0)]); assert.equal(retry.status,0); assert.match(retry.stdout,/test b \.\.\. ok/);
});

test('failed doctests prevent a shard from using incomplete preparation', async t => {
 const f=fixture(t),e=expandCohort({id:'rust-tests',tier:'full',title:'Rust tests',cards:[],cohort:'rust-debug',run:[{cmd:'cargo',args:['test','--workspace','--locked','--no-fail-fast']}]},{runDir:f.root});
 const prep=spawnSync(e.prepare.cmd,e.prepare.args,{cwd:f.root,env:{...f.env,BAD_DOC:'1'},encoding:'utf8'}); assert.notEqual(prep.status,0); assert.match(prep.stderr,/doctests exited/);
 const shard=spawnSync(e.shards[0].cmd,e.shards[0].args,{cwd:f.root,env:f.env,encoding:'utf8'}); assert.notEqual(shard.status,0);
});

test('unavailable Cargo configuration falls back to the complete original command', async t => {
 const f=fixture(t),calls=join(f.root,'fallback'); mkdirSync(join(f.root,'.cargo'));
 writeFileSync(join(f.root,'.cargo/config.toml'),"[env]\nFOO='bar'\n");
 writeFileSync(join(f.root,'bin/cargo'),`#!${process.execPath}\nimport {appendFileSync} from 'node:fs';appendFileSync(${JSON.stringify(calls)},JSON.stringify(process.argv.slice(2))+'\\n');console.log('test legacy_case ... ok');\n`,{mode:0o755});
 const step: GateStep={id:'rust-slow',tier:'full',title:'Rust slow',cards:[],cohort:'rust-debug',run:[{cmd:'cargo',args:['test','--workspace','--locked','--no-fail-fast','--','--ignored'],whenEnvMissing:{vars:['MISSING'],args:['--skip','external','--exact'],unrun:[{name:'external',reason:'missing'}]}}]};
 const e=expandCohort(step,{runDir:f.root}); assert.deepEqual(e.prepare.whenEnvMissing,step.run?.[0].whenEnvMissing);
 const opts={cwd:f.root,env:f.env,encoding:'utf8' as const},prep=spawnSync(e.prepare.cmd,[...e.prepare.args,'--skip','external','--exact'],opts); assert.equal(prep.status,0,prep.stderr);
 for(const cmd of e.shards) assert.equal(spawnSync(cmd.cmd,[...cmd.args,'--skip','external','--exact'],opts).status,0);
 assert.deepEqual(readFileSync(calls,'utf8').trim().split('\n').map((line): string[] => JSON.parse(line)),[['test','--workspace','--locked','--no-fail-fast','--','--ignored','--skip','external','--exact']]);
});

test('a failed fresh preparation removes the prior inventory', async t => {
 const f=fixture(t),e=expandCohort({id:'rust-tests',tier:'full',title:'Rust tests',cards:[],cohort:'rust-debug',run:[{cmd:'cargo',args:['test','--workspace','--locked','--no-fail-fast']}]},{runDir:f.root});
 const run=(c: GateCommand,env=f.env)=>spawnSync(c.cmd,c.args,{cwd:f.root,env,encoding:'utf8'});
 assert.equal(run(e.prepare).status,0); assert.notEqual(run(e.prepare,{...f.env,BAD_DOC:'1'}).status,0); assert.notEqual(run(e.shards[0]).status,0);
});


test('a failed shard keeps its binaries for retry when disk space is low', async t => {
 const f=fixture(t); writeFileSync(join(f.root,'bin/df'),`#!${process.execPath}\nconsole.log('Filesystem 1G-blocks Used Available Capacity Mounted\\nfixture 200 1 10 1% /');\n`,{mode:0o755});
 const e=expandCohort({id:'rust-tests',tier:'full',title:'Rust tests',cards:[],cohort:'rust-debug',run:[{cmd:'cargo',args:['test','--workspace','--locked','--no-fail-fast']}]},{runDir:f.root});
 const run=(c: GateCommand,env=f.env)=>spawnSync(c.cmd,c.args,{cwd:f.root,env,encoding:'utf8'}); assert.equal(run(e.prepare).status,0);
 const outcomes=e.shards.map(c=>run(c,{...f.env,FAIL_BINARY:'b'})); const failed=outcomes.findIndex(x=>x.status!==0);
 assert.ok(failed>=0); assert.equal(existsSync(join(f.target,'debug/deps/b-0123456789abcdef')),true);
 assert.equal(existsSync(join(f.target,'debug/deps/a-0123456789abcdef')),false);
 assert.equal(run(e.shards[failed]).status,0); assert.equal(existsSync(join(f.target,'debug/deps/b-0123456789abcdef')),false);
});


test('the debug hold spans the complete cohort and is released after a failure', async t => {
 const f=fixture(t); rmSync(join(f.target,'.rv-gate-cargo'));
 const init=spawnSync('git',['init','-q',f.root],{encoding:'utf8'}); assert.equal(init.status,0,init.stderr);
 await assert.rejects(withCohortHold('rust-debug',{cwd:f.root,target:f.target},async()=>{
  assert.equal(readFileSync(join(f.target,'.rv-gate-cargo'),'utf8').trim(),String(process.pid));
  assert.equal(readFileSync(join(f.target,'.rv-gate-cargo.profile'),'utf8').trim(),'debug');
  throw new Error('failed shard');
 }),/failed shard/);
 assert.equal(existsSync(join(f.target,'.rv-gate-cargo')),false);
});

test('a missing compiled test binary reports an environment retry instead of a test failure', async t => {
 const f=fixture(t),e=expandCohort({id:'rust-tests',tier:'full',title:'Rust tests',cards:[],cohort:'rust-debug',run:[{cmd:'cargo',args:['test','--workspace','--locked','--no-fail-fast']}]},{runDir:f.root});
 const run=(c: GateCommand)=>spawnSync(c.cmd,c.args,{cwd:f.root,env:f.env,encoding:'utf8'});assert.equal(run(e.prepare).status,0);
 rmSync(join(f.target,'debug/deps/a-0123456789abcdef'));
 const result=e.shards.map(run).find(item=>item.status!==0);assert.ok(result);assert.equal(result.status,75);assert.match(result.stderr,/environment-retry: missing test executable/);
});
