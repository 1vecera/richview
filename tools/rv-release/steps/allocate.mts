import {execFileSync} from 'node:child_process';
import {selected} from './route-choice.mts';
import {dirname,join} from 'node:path';
import {existsSync,readFileSync,writeFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import type { DescriptorInput } from '../lib/descriptor.mts';
const [root,route,sha,base,here]=process.argv.slice(2);
const desc=JSON.parse(readFileSync(process.env.RV_RELEASE_DESCRIPTOR!,'utf8')) as DescriptorInput, number=Number(desc.candidate.split('/')[1]);
if(!['A','B'].includes(route)||![sha,base].every(s=>/^[0-9a-f]{40}$/.test(s)))throw Error('Invalid cut selection.');
const {acquireLock,gateHome}=await import(pathToFileURL(join(root,'tools/rv-gate/lib/store.mjs')).href) as typeof import('../../rv-gate/lib/store.mts');
const typedProc=join(root,'tools/rv-gate/lib/proc.mts');
const {gateEnv}=await import(pathToFileURL(existsSync(typedProc)?typedProc:join(root,'tools/rv-gate/lib/proc.mjs')).href) as typeof import('../../rv-gate/lib/proc.mts');
const cwd=join(gateHome(),'work');
const git=(...args: string[])=>execFileSync('git',args,{cwd,encoding:'utf8',env:gateEnv({RV_GATE_PUSH:'candidate'})}).trim();
const routeLock=acquireLock('site-route-'+number);let unlock;
try{
 unlock=acquireLock('candidate',{wait:true});
 selected(dirname(here),route,sha);
 const refs=new Map(git('ls-remote','--heads','origin').split('\n').map(line=>line.split('\t').reverse() as [string, string]));
 if(refs.get('refs/heads/prod')!==base||refs.get('refs/heads/pre-prod')!==sha)throw Error('Reviewed pre-prod or private prod moved before allocation.');
 const file=join(here,'allocation.json');
 // The candidate named by the descriptor was cut at the reviewed commit before the release: adopt it, never allocate the next number.
 const numbers=[...refs.keys()].map(k=>/^refs\/heads\/candidate\/(\d+)$/.exec(k)).filter(m=>m!==null).map(m=>Number(m[1]));
 if(refs.get('refs/heads/'+desc.candidate)!==sha||Math.max(0,...numbers)!==number)throw Error('Expected '+desc.candidate+' at the reviewed commit and no later candidate.');
 const planned=existsSync(file)?JSON.parse(readFileSync(file,'utf8')) as {route: string; sha: string; base: string; candidate: string}: {route,sha,base,candidate:desc.candidate};
 if(planned.route!==route||planned.sha!==sha||planned.base!==base||!/^candidate\/[1-9]\d*$/.test(planned.candidate))throw Error('Conflicting candidate allocation');
 if(!existsSync(file))writeFileSync(file,JSON.stringify(planned)+'\n',{flag:'wx'});
 const ref='refs/heads/'+planned.candidate;
 if(!refs.has(ref))git('push','--force-with-lease='+ref+':','origin',sha+':'+ref);
 if(git('ls-remote','--heads','origin',ref)!==sha+'\t'+ref)throw Error('Candidate ref/lease conflict. Preserve the allocation journal.');
 console.log(planned.candidate);
}finally{unlock?.();routeLock();}
