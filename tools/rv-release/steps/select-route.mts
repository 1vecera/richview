import {execFileSync} from 'node:child_process';
import {existsSync,readFileSync,writeFileSync,renameSync} from 'node:fs';
import {randomUUID} from 'node:crypto';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {join} from 'node:path';
import {selected} from './route-choice.mts';
import {authorization,eligible,latestAnswer} from './windows/decision.mts';

import type { DescriptorInput } from '../lib/descriptor.mts';

const steps=fileURLToPath(new URL('.',import.meta.url)), root=join(steps,'..');
const [op,route,sha,arg]=process.argv.slice(2);
const desc=JSON.parse(readFileSync(process.env.RV_RELEASE_DESCRIPTOR!,'utf8')) as DescriptorInput, lockName='site-route-'+desc.candidate.split('/')[1];
const answer=latestAnswer();
console.log('Q55 '+(answer?answer.option+' at '+answer.at:'unanswered')+'. Eligible routes: '+eligible(answer));
if(op==='show')process.exit(0);
const target=op==='bind'?(arg??sha):sha;
// The release descriptor names the one route (B, the reviewed full) and its commit.
if(route!==desc.route)throw Error('This release has route '+desc.route+' only.');
if(sha!==desc.final)throw Error('Route '+desc.route+' requires the reviewed full commit '+desc.final+'.');
if(sha!==target)throw Error('Use the selected full SHA.');
function replace(path: string,bytes: string){
 const tmp=path+'.'+randomUUID()+'.tmp';writeFileSync(tmp,bytes,{flag:'wx',mode:0o600});renameSync(tmp,path);
}
function archive(record: ReturnType<typeof authorization>){
 const stamp=record.answer.at.replace(/[^a-zA-Z0-9]/g,'-');
 const path=join(root,['precut',route,sha,record.scope,record.answer.option,stamp].join('-')+'.json');
 const bytes=JSON.stringify(record)+'\n';
 if(!existsSync(path))writeFileSync(path,bytes,{flag:'wx',mode:0o600});
 if(readFileSync(path,'utf8')!==bytes)throw Error('Conflicting archived decision.');
}
const quote=(mode: string,id: string)=>execFileSync(process.execPath,[join(steps,'windows/cost.mjs'),join(root,'windows-'+id),mode,'quote',route],{stdio:'inherit'});
if(op==='select'){
 const repo=execFileSync('git',['-C',root,'rev-parse','--show-toplevel'],{encoding:'utf8'}).trim();
 const {acquireLock}=await import(pathToFileURL(join(repo,'tools/rv-gate/lib/store.mjs')).href) as typeof import('../../rv-gate/lib/store.mts');
 const release=acquireLock(lockName);
 try{
  const record=authorization(route,arg,sha,latestAnswer());quote(arg,sha);archive(record);
  replace(join(root,'selected-route.json'),JSON.stringify(record)+'\n');
 }finally{release();}
}else if(['check','bind'].includes(op)){
 const record=selected(root,route,sha);
 quote(record.scope,target);
 if(op==='bind'){
  const here=join(root,'go-record-'+target.slice(0,8)), decision=authorization(route,record.scope,target,record.answer);
  if(!readFileSync(join(here,'steps.tsv'),'utf8').split('\n').includes('FINAL\t'+target))throw Error('Journal SHA mismatch.');
  replace(join(here,'windows-budget-go.txt'),JSON.stringify(record.answer)+'\n');
  replace(join(here,'windows-ceiling.txt'),JSON.stringify(decision)+'\n');
  replace(join(here,'selected-route.json'),JSON.stringify(record)+'\n');
 }
}else throw Error('Use show, select, check or bind.');
