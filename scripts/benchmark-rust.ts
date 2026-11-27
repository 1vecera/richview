// Record the release engine's timings with its exact inputs and stable artifact signatures.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {mkdirSync,readFileSync,readdirSync,statSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {root} from './browser-harness.ts';

process.chdir(root);
execFileSync('cargo',['build','--release','--locked','-p','richview-bench'],{stdio:'inherit'});
const binary='target/release/richview-bench';
const inputs=Object.fromEntries(['small','six-types','dense','flagship'].map(name=>{
 const path=`tmp/performance/fixtures/${name}/report.rmd`;
 return [name,{path,source_sha256:createHash('sha256').update(readFileSync(path)).digest('hex')}];
}));
function files(folder: string): string[] {return readdirSync(folder,{withFileTypes:true}).flatMap(entry=>{
 if(entry.name==='Rendered')return [];
 const path=join(folder,entry.name);return entry.isDirectory()?files(path):[path];
});}
const paths=[binary,'dist/web/richview_site_bg.wasm',...files('tmp/performance/fixtures')].sort();
const snapshot=()=>Object.fromEntries(paths.map(path=>{
 const stat=statSync(path);
 return [path,{bytes:stat.size,mtime_ms:stat.mtimeMs,sha256:createHash('sha256').update(readFileSync(path)).digest('hex')}];
}));
const before=snapshot();
// The native executable owns the benchmark result. This driver forwards its measurements unchanged.
const result: Record<string, unknown> = JSON.parse(execFileSync(binary,[],{env:{...process.env,RICHVIEW_BENCH_ITERATIONS:process.env.RICHVIEW_BENCH_ITERATIONS??'100'},encoding:'utf8',maxBuffer:16<<20}));
assert.deepEqual(snapshot(),before,'Native benchmark inputs and artifacts changed during the capture');
result.provenance={captured_at:new Date().toISOString(),commit:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),inputs,artifacts:before,artifact_signatures_stable:true};
mkdirSync('tmp/performance',{recursive:true});
writeFileSync('tmp/performance/rust.json',JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify({results:result.results,peak_rss:result.peak_rss,artifact_signatures_stable:true}));
