import {createHash,randomUUID} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {existsSync,readdirSync,readFileSync,unlinkSync,writeFileSync} from 'node:fs';
import {dirname,join,resolve} from 'node:path';
// The release descriptor names the candidate this release deploys, the candidate it replaces and that deployment's history (DEL-83).
import type { DescriptorInput } from '../lib/descriptor.mts';

export type Deployment = { id: string; created_on: string; versions: {percentage: number; version_id: string}[]; annotations?: Record<string, string> };
type Binding = {sha: string; id: string; candidate: string};
type Attempt = Binding & {prior: string; priorDeployment: string; message: string};
type Observation = {attempt: string; deployment: string; from: string};
type Restoration = {attempt: string; predecessor: string; deployed: string; restored: string};
type Reconciliation = {current: string; from: string | null; pending?: boolean; attempt?: string; restoration?: Restoration | null};

const descriptorPath = process.env.RV_RELEASE_DESCRIPTOR!;
const desc=JSON.parse(readFileSync(descriptorPath,'utf8')) as DescriptorInput;
const releaseDir=desc.dir??dirname(resolve(descriptorPath));
const repoDir=process.env.RV_REPO??process.cwd();
const baselineSha=desc.old, fullSha=desc.final, baselineWorker=desc.oldWorker, baselineDeployment=desc.worker.baselineDeployment;
const baselineFirstWorker=desc.worker.firstWorker, baselineFirstDeployment=desc.worker.firstDeployment;
const baselineManifestSha256=desc.worker.manifestSha256; // M of the previous candidate's journal
export const priorFor=(sha: string)=>{if(sha!==fullSha)throw Error('This release deploys '+desc.candidate+' only.');return baselineWorker;};
const uuid=/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
export function sole(row: Deployment){
 const versions=row?.versions;
 if(!uuid.test(row?.id)||versions?.length!==1||versions[0].percentage!==100||!uuid.test(versions[0].version_id))throw Error('Cannot identify a single Worker version serving 100% of traffic.');
 return versions[0].version_id;
}
export function history(wr: string,cfg: string){
 const rows=(JSON.parse(execFileSync(wr,['deployments','list','--json','--config',cfg],{encoding:'utf8'})) as Deployment[]).sort((a,b)=>Date.parse(b.created_on)-Date.parse(a.created_on));
 if(!rows.length||rows.some((r,i)=>!Number.isFinite(Date.parse(r.created_on))||(i&&Date.parse(r.created_on)===Date.parse(rows[i-1].created_on))))throw Error('Empty or ambiguous deployment history.');
 return rows;
}
// Authenticate the completed deployment of the previous candidate as this release's starting point.
export function checkBaseline(release: string,rows: Deployment[]){
 const dir=join(release,'..',desc.previous.release,desc.previous.journal);
 const json=<T,>(f: string): T=>JSON.parse(readFileSync(f,'utf8'));
 const attempt=json<Attempt>(join(dir,'worker-attempt.json')), seen=json<Observation>(join(dir,'worker-history-observed.json'));
 const deploy=json<{candidate: string; commit: string; result: string; http: string; version: string; before?: {version: string}}>(join(repoDir,desc.previous.siteDeploy));
 const journal=readFileSync(join(dir,'steps.tsv'),'utf8').split('\n');
 const row=rows.find(r=>r.id===baselineDeployment);
 if(!row||sole(row)!==baselineWorker||seen.deployment!==row.id||seen.from!==baselineWorker||!attempt.message||attempt.message!==seen.attempt||
    attempt.sha!==baselineSha||attempt.candidate!==desc.oldCandidate||attempt.prior!==baselineFirstWorker||attempt.priorDeployment!==baselineFirstDeployment||
    deploy.candidate!==desc.oldCandidate||deploy.commit!==baselineSha||deploy.result!=='pass'||deploy.http!=='pass'||deploy.version!==baselineWorker||deploy.before?.version!==baselineFirstWorker||
    !journal.includes('NEW_WORKER\t'+baselineWorker)||!journal.includes('ok-41-deploy\tyes')||!journal.includes('ok-42-verify\tyes')||
    existsSync(join(dir,'worker-awaiting-history'))||existsSync(join(dir,'worker-restored.json')))throw Error('The previous candidate\'s deployment evidence differs from the reviewed baseline.');
}
export function checkPredecessor(release: string,rows: Deployment[],sha: string,markLegacyPending=false){
 const row=rows[0], prior=priorFor(sha), full=sha===fullSha;
 if(full)checkBaseline(release,rows);
 if(sole(row)!==prior)throw Error('Actual Worker writer found a different prior Worker. Hold.');
 const cycles=new Map<string, string>();
 for(const entry of readdirSync(release).filter(name=>/^go-record-[0-9a-f]{8}$/.test(name))){
  const dir=join(release,entry), file=join(dir,'worker-attempt.json');
  if(!existsSync(file))continue;
  const attempt=JSON.parse(readFileSync(file,'utf8')) as Attempt;
  const index=rows.findIndex(r=>r.id===attempt.priorDeployment);
  if(markLegacyPending && !full && index===0 && !existsSync(join(dir,'worker-awaiting-history')))writeFileSync(join(dir,'worker-awaiting-history'),attempt.message+'\n',{flag:'wx',mode:0o600});
  if(index<2 || existsSync(join(dir,'worker-awaiting-history')))throw Error('Worker attempt is pending in '+dir+'. Fresh history must show its deployment and restoration.');
  const seen=join(dir,'worker-history-observed.json'), restored=join(dir,'worker-restored.json');
  if(!existsSync(seen)||!existsSync(restored))throw Error('Inspect Worker recovery in '+dir+' to verify the restored homepage and deployment.');
  const observation=JSON.parse(readFileSync(seen,'utf8')) as Observation, proof=JSON.parse(readFileSync(restored,'utf8')) as Restoration;
  const deployed=rows[index-1], restore=rows[index-2];
  if(attempt.prior!==prior || !attempt.message || observation.attempt!==attempt.message || observation.deployment!==deployed.id ||
     observation.from===prior || observation.from!==sole(deployed) || sole(rows[index])!==prior || sole(restore)!==prior ||
     proof.attempt!==attempt.message || proof.predecessor!==attempt.priorDeployment || proof.deployed!==deployed.id || proof.restored!==restore.id){
   throw Error('Fresh Worker history differs from the recorded restoration in '+dir);
  }
  cycles.set(restore.id,attempt.priorDeployment);
 }
 if(cycles.size||full){
  // Every later attempt of this release must extend the preceding verified restoration.
  let cursor=row.id, matched=0;
  while(cycles.has(cursor)){const next=cycles.get(cursor)!;cycles.delete(cursor);cursor=next;matched++;}
  if((!matched&&!full)||cycles.size||(full&&cursor!==baselineDeployment))throw Error('Active Worker deployment does not end the verified restoration history.');
  return true;
 }
 return false;
}
export function arm(wr: string,cfg: string,here: string,binding: Binding){
 // The calling deployer holds the site lock across the history check and write.
 const release=dirname(here), rows=history(wr,cfg), row=rows[0], prior=priorFor(binding.sha);
 if(checkPredecessor(release,rows,binding.sha,true))verifyOldHome(prior);
 const message='richview-site '+randomUUID();
 writeFileSync(join(here,'worker-attempt.json'),JSON.stringify({...binding,prior,priorDeployment:row.id,message})+'\n',{flag:'wx',mode:0o600});
 return message;
}
export function reconcile(wr: string,cfg: string,here: string,binding: Binding): Reconciliation{
 const prior=priorFor(binding.sha);
 const rows=history(wr,cfg), current=sole(rows[0]), file=join(here,'worker-attempt.json');
 if(!existsSync(file)){
  if(current===prior)return {current,from:null};
  throw Error('New Worker without a local writer intent; inspect history.');
 }
 const attempt=JSON.parse(readFileSync(file,'utf8')) as Attempt;
 if(attempt.prior!==prior||Object.entries(binding).some(([k,v])=>attempt[k as keyof Binding]!==v))throw Error('Wrong Worker attempt binding.');
 const index=rows.findIndex(r=>r.id===attempt.priorDeployment);
 if(index<0||index>2||sole(rows[index])!==prior)throw Error('Reviewed predecessor missing, or other deployments intervened.');
 const pending=join(here,'worker-awaiting-history');
 if(index===0){
  if(!existsSync(pending))writeFileSync(pending,attempt.message+'\n',{flag:'wx',mode:0o600});
  return {current,from:null,pending:true,attempt:attempt.message};
 }
 const row=rows[index-1], from=sole(row);
 if(from===prior||(index===2&&current!==prior))throw Error('Unexpected third Worker or repeated deployment.');
 const version=JSON.parse(execFileSync(wr,['versions','view',from,'--json','--config',cfg],{encoding:'utf8'})) as {id: string; annotations?: Record<string,string>};
 if(version.id!==from||(version.annotations?.['workers/message']!==attempt.message&&row.annotations?.['workers/message']!==attempt.message))throw Error('Could not attribute the new Worker to this attempt. Hold.');
 const seen=join(here,'worker-history-observed.json');
 const observation=JSON.stringify({attempt:attempt.message,deployment:row.id,from})+'\n';
 if(!existsSync(seen))writeFileSync(seen,observation,{flag:'wx',mode:0o600});
 if(readFileSync(seen,'utf8')!==observation)throw Error('Deployment history conflicts with the earlier verified observation.');
 if(existsSync(pending))unlinkSync(pending);
 const restoration=index===2?{attempt:attempt.message,predecessor:attempt.priorDeployment,deployed:row.id,restored:rows[0].id}:null;
 return {current,from,restoration};
}
export function confirmRestored(here: string,result: Reconciliation,prior: string){
 if(result.current!==prior||result.pending||(result.from&&!result.restoration))throw Error('No confirmed restore deployment in Worker history.');
 verifyOldHome(prior);
 if(!result.restoration)return;
 const file=join(here,'worker-restored.json'), bytes=JSON.stringify(result.restoration)+'\n';
 if(!existsSync(file))writeFileSync(file,bytes,{flag:'wx',mode:0o600});
 if(readFileSync(file,'utf8')!==bytes)throw Error('A different restore deployment was previously recorded.');
}
// The kept build of the previous candidate may be gone to free disk. Its homepage digest comes from the full's
// retained build manifest, whose own SHA-256 is the M that release 30's journal recorded: the same authenticated source, one step removed.
export function verifyOldHome(prior: string){
 if(prior!==baselineWorker)throw Error('Unknown predecessor Worker.');
 const file=join(releaseDir,'..',desc.previous.release,desc.previous.retainedFull,'build-manifest.json');
 const bytes=readFileSync(file), digest=(data: Buffer)=>createHash('sha256').update(data).digest('hex');
 if(digest(bytes)!==baselineManifestSha256)throw Error('The retained manifest of the previous candidate differs from its journaled digest.');
 const manifest=JSON.parse(bytes.toString("utf8")) as {inputs?: {commit: string; dirty: boolean}; mode: string; outputs?: Record<string,{sha256?: string}>};
 if(manifest.inputs?.commit!==baselineSha||manifest.mode!=='full'||manifest.inputs?.dirty!==false)throw Error('The retained manifest of the previous candidate is not the clean full build.');
 const expected=manifest.outputs?.['checkout:dist/web/index.html']?.sha256;
 if(!/^[0-9a-f]{64}$/.test(expected??''))throw Error('The retained manifest of the previous candidate has no homepage digest.');
 const live=execFileSync('curl',['--proto','=https','-fsS','--max-time','30','https://richview.ai/']);
 console.log('Homepage observed '+live.length+' bytes, SHA-256 '+digest(live)+'; the previous candidate manifest says '+expected+'.');
 if(digest(live)!==expected)throw Error('Reviewed predecessor homepage bytes differ.');
}
