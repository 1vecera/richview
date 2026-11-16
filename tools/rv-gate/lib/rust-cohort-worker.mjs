// Host reason (ARC-03): run one held Cargo test cohort and its independently retriable shards.
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, renameSync, rmSync, writeFileSync, writeSync } from 'node:fs';
import { dirname, isAbsolute } from 'node:path';
import { cargoHolder, dropTestBinaries, KEEP_TEST_BINARIES_GIB } from './target.mjs';
import { freeGib } from './proc.mjs';
import { cargoContextNeedsOriginal, inventory, packageEnv, selection } from './rust-tests.mjs';

const [action,state,encoded,...extra] = process.argv.slice(2);
const args = JSON.parse(encoded), plan = selection(args), env=process.env, cwd=process.cwd();
class MissingTestBinary extends Error {}
function run(cmd, argv, options={}) {
 writeSync(1, `$ ${[cmd,...argv].join(' ')}\n`);
 const child=spawnSync(cmd,argv,{cwd,env,encoding:'utf8',maxBuffer:256*1024*1024,...options});
 if(child.stderr) process.stderr.write(child.stderr);
 if(child.error) throw child.error;
 return child;
}
function check(result, label) { if(result.status!==0) throw new Error(`${label} exited ${result.status ?? result.signal}`); return result.stdout; }
function save(value) { const tmp=`${state}.${process.pid}.tmp`; try {writeFileSync(tmp,JSON.stringify(value));renameSync(tmp,state);} finally {rmSync(tmp,{force:true});} }
try {
 if (!plan || !env.CARGO_TARGET_DIR || !/^\d+$/.test(env.RV_GATE_HOLDER ?? '') || cargoHolder(env.CARGO_TARGET_DIR)!==Number(env.RV_GATE_HOLDER)) throw new Error('Rust cohort requires its owning Cargo hold');
 if (action==='prepare') {
  rmSync(state,{force:true});
  if (cargoContextNeedsOriginal(cwd,env)) {
   const original=run('cargo',[...args,...extra]); if(original.stdout) process.stdout.write(original.stdout); check(original,'Cargo test'); save({fallback:true,args});
  }
  else {
   const events=check(run('cargo',plan.compile),'Cargo compile').trim().split('\n').filter(Boolean).map(JSON.parse);
   const metadata=JSON.parse(check(run('cargo',['metadata','--locked','--no-deps','--format-version=1']),'Cargo metadata'));
   if(metadata.packages.some(pkg=>pkg.targets.some(target=>target.test && target['required-features']?.length))) {
    const original=run('cargo',[...args,...extra]); if(original.stdout) process.stdout.write(original.stdout); check(original,'Cargo test'); save({fallback:true,args});
   }
   else {
    const binaries=inventory(events,metadata,env.CARGO_TARGET_DIR);
    const stdlib=check(run(env.RUSTC || 'rustc',['--print','target-libdir']),'rustc').trim();
    if(!isAbsolute(stdlib)) throw new Error('Invalid Rust library path');
    const docs=run('cargo',[...plan.docs,...extra]); if(docs.stdout) process.stdout.write(docs.stdout); check(docs,'Cargo doctests');
    const names=new Set([...(docs.stderr ?? '').matchAll(/^\s*Doc-tests (\S+)\s*$/gm)].map(match=>match[1]));
    for(const pkg of metadata.packages.filter(pkg=>metadata.workspace_members.includes(pkg.id))) for(const target of pkg.targets.filter(target=>target.doctest)) if(!names.has(target.name.replaceAll('-','_'))) throw new Error(`Missing doctest cohort ${target.name}`);
    save({args,events,metadata,binaries,stdlib});
   }
  }
 } else if (action==='shard') {
  const [indexRaw,countRaw,...added]=extra, index=Number(indexRaw),count=Number(countRaw);
  if(!Number.isSafeInteger(index)||!Number.isSafeInteger(count)||index<0||index>=count||count!==4) throw new Error('Invalid Rust shard');
  const cohort=JSON.parse(readFileSync(state,'utf8'));
  if(JSON.stringify(cohort.args)!==JSON.stringify(args)) throw new Error('Rust cohort does not match the command');
  if(cohort.fallback) {
   // Preparation ran the original complete Cargo command.
  } else {
   let failed=false; const completed=[];
   for(const [n,artifact] of cohort.binaries.entries()) if(n%count===index) {
    if(!existsSync(artifact.executable)) throw new MissingTestBinary(artifact.executable);
    let result;
    try { result=run(artifact.executable,[...plan.harness,...added],{cwd:dirname(artifact.pkg.manifest_path),env:packageEnv(env,artifact.pkg,artifact,cohort.events,cohort.stdlib,cohort.metadata.target_directory),stdio:'inherit'}); }
    catch(error) { if(error.code==='ENOENT' && !existsSync(artifact.executable)) throw new MissingTestBinary(artifact.executable); throw error; }
    if(result.stdout) process.stdout.write(result.stdout); if(result.status!==0) failed=true;
    completed.push(artifact.executable);
   }
   if(failed) throw new Error('Rust test binary failed');
   // Failed partitions retain every binary for their in-run retry.
   const dropped=dropTestBinaries(env.CARGO_TARGET_DIR,completed,freeGib());
   if(dropped.count) console.log(`rv-gate Rust cohort: deleted ${dropped.count} completed test binaries (${(dropped.bytes/2**30).toFixed(1)} GiB), with / under ${KEEP_TEST_BINARIES_GIB} GiB free`);
  }
 } else throw new Error('Unknown Rust cohort action');
} catch(error) {
 if(error instanceof MissingTestBinary) {console.error(`rv-gate Rust cohort environment-retry: missing test executable ${error.message}`);process.exitCode=75;}
 else {console.error(`rv-gate Rust cohort: ${error.message}`);process.exitCode=1;}
}
