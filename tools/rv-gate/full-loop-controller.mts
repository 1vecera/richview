// Host reason (ARC-03): run the organizer's host-local loop over frozen dev.
// The installed run-full-loop.mjs entry imports this file beside its host-local helpers.
import fs from 'node:fs';
import {spawn,execFileSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';
import {homedir} from 'node:os';
type BuildTools=Pick<typeof import('./lib/target.mts'),'keepReleaseBuild'|'leaseReleaseBuild'>;
type KeepOptions=BuildTools & {target:string;cwd:string;commit:string;manifest:unknown;dir:string;keep:number};
type HostPromotion={
 promoteWithCurrentRunner(options:{launcher:string;gate:string;receiptId:string;apply:boolean}):Record<string,unknown>;
 promoteByLeadJudgment(options:{launcher:string;sourceDir:string;head:string;receiptId:string;log:string;apply:boolean}):Record<string,unknown> & {updated:boolean};
};
type ReceiptStep={id:string;status:string;reason?:string};
type FullReceipt={id:string;at:string;commit:string;tree:string;result:string;cancelled?:boolean;steps:ReceiptStep[];browserRelaunchTelemetry?:{status:string;reason?:string};outcomes?:Record<string,string>};
type HistoryEvent={head:string;result:string};
type Refusal={kind:string;head:string;receipt?:string;reason:string};
type Hold={reason:string;last?:string;due?:string};
type ActiveRun={
 state:string;head:string;log:string;receipt?:string;childPid?:number;adoptedPids?:number[];exitCode?:number|null;
 mode?:string;parallel?:number;started?:string;controllerPid?:number;resourceAdmission?:{freeGiB:number;memoryFreePercent:number|null;policy:string};baselineAcceptance?:string;
};
type JudgmentState={promoted?:boolean;kept?:boolean;keepError?:string;error?:string};
type PerfAcceptance={id:string} & Record<string,unknown>;

// These helpers live beside the installed controller on the build host.
const {fullBudget}=await import(new URL('./full-loop-policy.mjs',import.meta.url).href) as {fullBudget(freeGiB:number,memoryFreePercent:number|null):number};
const {keepFullBuild}=await import(new URL('./full-loop-keep.mjs',import.meta.url).href) as {keepFullBuild(options:KeepOptions):{kept:unknown;verifiedOutputHashes:number}};
const {promoteWithCurrentRunner,promoteByLeadJudgment}=await import(new URL('./controller-promotion.mjs',import.meta.url).href) as HostPromotion;
const root=`${homedir()}/tmp/richview-rewrite`;
// Outside .claude/worktrees since 5 Nov, so no worktree cleaner reaches it (a cleanup removed it on 4 Nov
// and no full ran for 14 h). Still a locked worktree of the main repo, on its own fix branch.
const launcher=`${root}/launcher`;
// The frozen launcher can still be on the prior JavaScript toolchain during handoff.
const launcherModule=(name:string)=>{
 const base=launcher+'/tools/rv-gate/lib/'+name;
 return pathToFileURL(base+(fs.existsSync(base+'.mts')?'.mts':'.mjs')).href;
};
const {readDev}=await import(launcherModule('pre-prod')) as Pick<typeof import('./lib/pre-prod.mts'),'readDev'>;
const {acquireLock,gateHome,receiptNotes}=await import(launcherModule('store')) as Pick<typeof import('./lib/store.mts'),'acquireLock'|'gateHome'|'receiptNotes'>;
const {keepReleaseBuild,leaseReleaseBuild}=await import(launcherModule('target')) as BuildTools;
const git=(...args: string[])=>execFileSync('git',args,{cwd:launcher,encoding:'utf8'}).trim();
const pause=(ms: number|undefined)=>new Promise(resolve=>setTimeout(resolve,ms));
const alive=(pid: number)=>{try{process.kill(pid,0);return true;}catch{return false;}};
const save=(file:string,value:unknown)=>{const temp=`${file}.${process.pid}.tmp`;fs.writeFileSync(temp,JSON.stringify(value,null,2)+'\n');fs.renameSync(temp,file);};
const stopFile=`${root}/full-loop.stop`;
const activeFile=`${root}/active-full.json`;
if(!process.argv.includes('--run')){console.log(JSON.stringify({mode:'read-only plan',launcher,adopts:activeFile,stopFile,target:'target',start:'only at >=15GiB free',parallel:'at most 2;1 when known memory<25%;refuse below 15GiB',actions:['read dev without moving refs','hold a head after strict promotion or two non-cancelled fails until dev moves or a day after its last run','run affected in prelaunch, full in launch','keep verified build','advance pre-prod on build and site by lead judgment','record and route failures to lead','record strict passing proof separately','repeat'],prod:'never written'}));process.exit(0);}
// Since 3 Nov the loop builds in the one shared target (Daniel, via the chief of staff); target-release is gone.
if(process.env.CARGO_TARGET_DIR!==`${root}/target`)throw Error('Source env.sh before starting the loop');
let stopping=false;
// One mail per distinct infrastructure error until a run completes: a missing node path mailed every 30 s on 3 Nov.
let lastInfra:string|null=null;
process.on('SIGINT',()=>{stopping=true;});process.on('SIGTERM',()=>{stopping=true;});
const stopped=()=>stopping||fs.existsSync(stopFile);
const notify=(who: string,text: string,urgent?: string)=>{
 try{execFileSync('rv-mail',['send',who,text,'--from','richview-rewrite',...(urgent?['--urgent',urgent]:[])],{encoding:'utf8',stdio:['ignore','pipe','pipe'],timeout:5000});}
 catch(error){fs.appendFileSync(`${root}/ledger.md`,`\nActionable mail to ${who} failed: ${(error as Error).message}. Inspect the recorded full evidence; no verdict changed.\n`);}
};
// Loop trouble goes to the lead since 5 Nov: `richview-rewrite` is the retired release agent, aliased to rv-build.
const LEAD='richview-lockdown';
// A red full goes to whoever landed the commits since the last passing full, plus the lead (5 Nov: the old
// per-step owners were retired streams aliased to rv-build, which saw the notice 3 h late). A commit's owner
// is the implementer whose stream file names one of its S27 issues; a commit nobody claims goes to every implementer.
function redRecipients(head:string){
 const history=fs.readFileSync(`${root}/full-loop-history.jsonl`,'utf8').trim().split('\n').map(line=>{try{return JSON.parse(line) as HistoryEvent;}catch{return null;}}).filter(event=>event!==null);
 const base=history.filter(event=>event.result==='pass').at(-1)?.head;
 let commits:{hash:string;subject:string;issues:string[]}[]=[];
 try{commits=git('log','--format=%H%x1f%s%x1f%b%x1e',base?`${base}..${head}`:'-5',...(base?[]:[head])).split('\x1e').map(entry=>entry.trim()).filter(Boolean).map(entry=>{const [hash,subject,body]=entry.split('\x1f');return {hash,subject,issues:[...new Set(`${subject} ${body}`.match(/S27-\d+/g)??[])]};});}catch{}
 const list=commits.map(commit=>`${commit.hash} ${commit.subject.slice(0,70)}${commit.issues.length?` [${commit.issues.join(',')}]`:''}`).join('; ');
 return {base,list};
}
function perfAcceptance(head:string){
 const file=root+'/perf-baseline-acceptance.json';
 if(!fs.existsSync(file))return undefined;
 const request=JSON.parse(fs.readFileSync(file,'utf8')) as PerfAcceptance;
 const baselineFile=gateHome()+'/perf-baseline.json';
 const baseline=fs.existsSync(baselineFile)?JSON.parse(fs.readFileSync(baselineFile,'utf8')) as {acceptedGrowth?:{id:string}[]}:null;
 if(baseline?.acceptedGrowth?.some(record=>record.id===request.id))return undefined;
 return {...request,commit:head};
}
function devHead(){
 const source=readDev(launcher);
 save(`${root}/dev-check-current.json`,{time:new Date().toISOString(),...source,mode:'read-only source selection; no tracking'});
 return source;
}
// A head is not run back to back once its fulls have said what they can: after a pass, or after two
// fails in a row (a rerun separates a flake from a real red), the next full waits for dev to move, or
// reruns the same head once a day after its last full, for flake detection (lead, 3 Nov 09:55).
const RERUN_AFTER_MS=24*3600*1000;
function registrationRefusal(head:string):Refusal|null{
 const file=`${root}/full-loop-status.json`;
 if(!fs.existsSync(file))return null;
 const status=JSON.parse(fs.readFileSync(file,'utf8')) as Refusal;
 return status.kind==='registration-refused'&&status.head===head?status:null;
}
function headHold(head:string,mode:string,base:string):Hold|null{
 const status=registrationRefusal(head);
 if(status)return {reason:status.reason,last:status.receipt};
 if(head===base)return {reason:'pre-prod already equals dev',last:head};
 const dir=`${gateHome()}/evidence/receipts`,suffix=`-${head.slice(0,10)}.json`;
 const fulls=fs.readdirSync(dir).filter(name=>new RegExp(`^${mode}-\\d{8}T\\d{6}Z-[0-9a-f]+\\.json$`).test(name)&&name.endsWith(suffix)).sort()
  .map(name=>{try{return JSON.parse(fs.readFileSync(`${dir}/${name}`,'utf8')) as FullReceipt;}catch{return null;}})
  .filter((receipt):receipt is FullReceipt=>receipt?.commit===head&&!receipt.cancelled);
 const passed=fulls.filter(receipt=>receipt.result==='pass'&&receiptNotes(receipt.id).some(note=>note.kind==='promotion'&&note.commit===head&&note.tree===receipt.tree)).at(-1);
 const failedTwice=fulls.length>=2&&fulls.slice(-2).every(receipt=>receipt.result!=='pass');
 if(!passed&&!failedTwice)return null;
 const last=fulls.at(-1)!,due=Date.parse(last.at)+RERUN_AFTER_MS;
 const reason=passed?`it passed and promoted ${passed.id}`:`its last two ${mode} runs failed (${fulls.at(-2)!.id}, ${last.id})`;
 return Date.now()<due?{reason,last:last.id,due:new Date(due).toISOString()}:null;
}
function promote(active:ActiveRun,event:{result:string}){
 if(event.result!=='pass')return;
 const refuse=(reason: string)=>{
  const status={kind:'registration-refused',head:active.head,receipt:active.receipt,reason,at:new Date().toISOString()};
  save(`${root}/full-loop-status.json`,status);
  const note=`\nStrict passing proof registration refused: ${reason}; holding ${active.head} until dev moves. Recorded judgment status is unchanged.\n`;
  fs.appendFileSync(`${root}/ledger.md`,note);notify(LEAD,note);console.log(note);
 };
 // Never move pre-prod without a kept, verified build of the same head: preprod.richview.ai and the hotfix lane deploy it (lead, 9 Nov).
 const keptDir=`${root}/bin-release/${active.head.slice(0,10)}`;
 if(!fs.existsSync(`${keptDir}/dist/web`)||!fs.existsSync(`${keptDir}/build-manifest.json`)){refuse(`No kept build of ${active.head.slice(0,10)} exists in bin-release`);return;}
 try{
  const promotion=promoteWithCurrentRunner({launcher,gate:gateHome()+'/gate',receiptId:active.receipt!,apply:true});
  const record={at:new Date().toISOString(),...promotion,prod:git('ls-remote','origin','refs/heads/prod').split(/\s/)[0]};
  save(`${root}/promotion-current.json`,record);
  const note='\nPassing full promoted exact checked source into pre-prod: '+JSON.stringify(record)+'. Prod untouched.\n';
  fs.appendFileSync(`${root}/ledger.md`,note);console.log(note);
 }catch(error){refuse((error as Error).message);}
}
function changedFailures(steps:ReceiptStep[],receiptId:string|undefined,previous:FullReceipt|null){
 const first=(step:ReceiptStep,id?:string)=>String(step.reason??'').split('\n')[0].replaceAll(id||'\0','<receipt>');
 const before=new Map((previous?.steps??[]).filter((step: { status: string; })=>step.status==='fail').map(step=>[step.id,first(step,previous!.id)]));
 return steps.filter(step=>step.status==='fail'&&before.get(step.id)!==first(step,receiptId));
}
function previousFullReceipt(){
 try{
  const history=fs.readFileSync(`${root}/full-loop-history.jsonl`,'utf8').trim().split('\n');
  const last=JSON.parse(history.at(-1)!) as {receipt?:string};
  if(!/^(?:full|affected)-[A-Za-z0-9-]+$/.test(last.receipt??''))return null;
  return JSON.parse(fs.readFileSync(`${gateHome()}/evidence/receipts/${last.receipt}.json`,'utf8')) as FullReceipt;
 }catch{return null;}
}
function outcome(active:ActiveRun,code:number|null){
 const receipt=active.receipt&&`${gateHome()}/evidence/receipts/${active.receipt}.json`;
 const data=receipt&&fs.existsSync(receipt)?JSON.parse(fs.readFileSync(receipt,'utf8')) as FullReceipt:null;
 const failures=data?.steps.filter((step: { status: string; })=>step.status==='fail')??[];
 const changed=changedFailures(failures,active.receipt,previousFullReceipt());
 const event={at:new Date().toISOString(),head:active.head,receipt:active.receipt,log:active.log,exitCode:code,resourceAdmission:active.resourceAdmission,result:data?.result??'no receipt',cancelled:!!data?.cancelled,failed:failures.map(step=>step.id),changedFailures:changed.map(step=>step.id),unrun:data?.steps.filter((step: { status: string; })=>step.status==='unrun').map(step=>step.id)??[]};
 fs.appendFileSync(`${root}/full-loop-history.jsonl`,JSON.stringify(event)+'\n');
 const note=`\nFull loop completed ${JSON.stringify(event)}. Frozen source only; no prod move or green newer-head claim.\n`;
 fs.appendFileSync(`${root}/ledger.md`,note);
 if(data?.result==='fail'&&!data.cancelled){
  const {base,list}=redRecipients(active.head);
  const telemetry=data.browserRelaunchTelemetry;
  const incomplete=Object.entries(data.outcomes??{}).filter(([,status])=>status!=='pass');
  const reasons=[...failures.map(step=>`${step.id} (${String(step.reason??'').slice(0,160)})`),
   ...(telemetry&&telemetry.status!=='pass'?[`browser telemetry ${telemetry.status}: ${telemetry.reason??'no reason recorded'}`]:[]),
   ...(incomplete.length?[`incomplete affected units: ${incomplete.map(([id,status])=>`${id}=${status}`).join(', ')}`]:[])];
  const text=`The affected/full run is red: ${active.receipt} at ${active.head}; failures: ${reasons.join('; ')||'receipt reports failure without a failed step or recorded reason'}. ${base?`Changed range ${base}..${active.head}`:'Last passing base unavailable; recent commits only'}: ${list||'unknown'}. Step logs: ${gateHome()}/evidence/runs/${active.receipt}/logs; receipt ${receipt}.`;
  notify(LEAD,text);
 }else if(!data){
  const key=`no-receipt:${active.head}`;
  if(key!==lastInfra){const {base,list}=redRecipients(active.head);notify(LEAD,`Run ended without a receipt at ${active.head}; exit ${code??'unknown'}, log ${active.log}. ${base?`Changed range ${base}..${active.head}`:'Last passing base unavailable; recent commits only'}: ${list||'unknown'}.`);lastInfra=key;}
 }
 console.log(`Completed ${active.receipt??active.head}: ${event.result}; failed ${event.failed.join(',')||'none'}`);return event;
}
function keep(active:ActiveRun){
 const cwd=gateHome()+'/work',target=fs.realpathSync(cwd+'/target'),run=gateHome()+'/evidence/runs/'+active.receipt;
 const manifest=JSON.parse(fs.readFileSync(run+'/build-manifest.json','utf8')) as unknown;
 const kept=keepFullBuild({target,cwd,commit:active.head,manifest,dir:root+'/bin-release',keep:4,keepReleaseBuild,leaseReleaseBuild});
 const record={...kept,receipt:active.receipt};save(root+'/current-kept-build.json',record);
 const note='\nFull loop kept clean verified build '+JSON.stringify(record)+'. Run still in progress; no overall green claim or artifact substitution.\n';
 fs.appendFileSync(`${root}/ledger.md`,note);console.log('Kept '+kept.kept+': '+kept.verifiedOutputHashes+' output hashes verified');
}
// Called only after output reaches the run log, including while adopting a live run.
function considerJudgment(active:ActiveRun,state:JudgmentState){
 if(stopped()||state.promoted||!active.log||!fs.existsSync(active.log))return;
 const output=fs.readFileSync(active.log,'utf8');
 const match=/^rv-gate (?:affected|full): \S+ at ([0-9a-f]{10}); receipt ((?:affected|full)-[A-Za-z0-9-]+); parallel \d+$/m.exec(output);
 if(!match||match[1]!==active.head.slice(0,10))return;
 if(active.receipt&&active.receipt!==match[2])throw Error('Active run receipt differs from its log');
 if(!active.receipt){active.receipt=match[2];save(activeFile,active);}
 if(!/^(?:PASS|FLAKY)\s+build\s/m.test(output))return;
 if(!state.kept){try{keep(active);state.kept=true;}catch(error){if(state.keepError!==(error as Error).message){state.keepError=(error as Error).message;notify(LEAD,`Keep failed for ${active.head}: ${(error as Error).message}`);}return;}}
 if(!/^(?:PASS|FLAKY)\s+site\s/m.test(output))return;
 try{
  const promotion=promoteByLeadJudgment({launcher,sourceDir:gateHome()+'/work',head:active.head,receiptId:active.receipt,log:active.log,apply:true});
  state.promoted=true;save(root+'/promotion-current.json',{at:new Date().toISOString(),...promotion});
  console.log(`Lead judgment pre-prod ${promotion.updated?'advanced':'already covered'} ${active.head}; strict passing proof remains separate.`);
 }catch(error){if(state.error!==(error as Error).message){state.error=(error as Error).message;notify(LEAD,`Lead judgment refused for ${active.head}, ${active.receipt}: ${(error as Error).message}`);}}
}
function recordAdoptedCompleted(active:ActiveRun){
 active.state='completed';active.exitCode=null;save(activeFile,active);
}
if(process.argv.includes('--resume')){
 const previous=fs.existsSync(`${root}/full-loop.json`)?JSON.parse(fs.readFileSync(`${root}/full-loop.json`,'utf8')) as {pid?:number}:null;
 const stopStamp=fs.existsSync(stopFile)?fs.statSync(stopFile).mtimeMs:null;
 if(previous?.pid&&alive(previous.pid)){
  console.log('Waiting for prior controller '+previous.pid+' to finish its active full; no refs will move.');
  while(alive(previous.pid)&&!stopping)await pause(5000);
 }
 if(stopping)process.exit(0);
 if(fs.existsSync(stopFile)&&stopStamp!==fs.statSync(stopFile).mtimeMs)throw Error('Stop request changed during handoff; keep it and inspect');
 fs.rmSync(stopFile,{force:true});
}
// Release builds that waited out the last full get their turn before the next one starts (lead, 8 Nov 2026: the full
// on 057e3a43 started the moment db870625's ended, and a site build waited through a second whole run).
// The shim's waiters poll the release hold every 5 s and then take a ticket, so pause 15 s, then wait while any
// live process holds a release ticket, at most 30 minutes, so a stuck ticket never stops the loop.
async function drainReleaseQueue(){
 const queue=`${process.env.CARGO_TARGET_DIR??root+'/target'}/.rv-queue-release`,deadline=Date.now()+30*60000;
 const waiting=()=>(fs.existsSync(queue)?fs.readdirSync(queue):[]).filter(name=>alive(Number(name.split('-').pop())));
 await pause(15000);
 let told=false;
 while(Date.now()<deadline&&!stopped()){
  const live=waiting();
  if(!live.length)return;
  if(!told){console.log(`Letting ${live.length} queued release build(s) run before the next full`);told=true;}
  await pause(15000);
 }
}
const release=acquireLock('full-loop',{wait:false});
try{
 save(`${root}/full-loop.json`,{pid:process.pid,started:new Date().toISOString(),launcher,target:process.env.CARGO_TARGET_DIR,stopFile});
 if(typeof leaseReleaseBuild!=='function')throw Error('Kept-build reader lease API is unavailable');
 save(root+'/build-lease-activation.json',{state:'active',at:new Date().toISOString(),controllerPid:process.pid,launcher,source:git('rev-parse','HEAD'),scope:'This controller and current gate keepers use guarded retention; consumers must own a process-bound lease and verify their hashes',legacyPrunerRetired:true});
 const prior=fs.existsSync(activeFile)?JSON.parse(fs.readFileSync(activeFile,'utf8')) as ActiveRun:null;
 const adopted=prior?(prior.adoptedPids??[prior.childPid].filter((pid):pid is number=>Boolean(pid))):[];
 if(prior&&adopted.some(alive)){
  console.log(`Adopting current full ${prior.receipt}: pids ${adopted.join(',')}`);
  const state={};
  while(adopted.some(alive)&&!stopped()){considerJudgment(prior,state);await pause(5000);}
  if(!adopted.some(alive))recordAdoptedCompleted(prior);
  if(stopped())process.exitCode=0;else{considerJudgment(prior,state);const event=outcome(prior,null);promote(prior,event);}
 }else if(prior?.state==='running'){
  recordAdoptedCompleted(prior);
  const file=prior.receipt&&`${gateHome()}/evidence/receipts/${prior.receipt}.json`;
  if(!file||!fs.existsSync(file)){
   notify(LEAD,`Cannot reconcile prior active run ${prior.receipt??'without a receipt'} at ${prior.head}; holding rather than starting a duplicate.`);
   while(!stopped())await pause(30000);
  }else{
   considerJudgment(prior,{});const event=outcome(prior,null);promote(prior,event);
  }
 }else if(prior?.state==='completed'&&prior.exitCode===null&&(!prior.receipt||!fs.existsSync(`${gateHome()}/evidence/receipts/${prior.receipt}.json`))){
  notify(LEAD,`Cannot reconcile completed adopted run ${prior.receipt??'without a receipt'} at ${prior.head}; holding rather than starting a duplicate.`);
  while(!stopped())await pause(30000);
 }else if(prior?.state==='completed'&&(prior.exitCode===0||prior.exitCode===null)&&prior.receipt){
  const file=`${gateHome()}/evidence/receipts/${prior.receipt}.json`;
  if(fs.existsSync(file)&&JSON.parse(fs.readFileSync(file,'utf8')).result==='pass'){
   const noted=receiptNotes(prior.receipt).some((note: { kind: string; })=>note.kind==='promotion');
   if(noted)console.log('Completed passing full already has its strict promotion proof: '+prior.receipt);
   else if(registrationRefusal(prior.head))console.log('Completed passing full already has a registration refusal: '+prior.receipt);
   else{
    console.log('Reconciling completed passing full before starting another source: '+prior.receipt);
    considerJudgment(prior,{});
    promote(prior,{result:'pass'});
   }
  }
 }
 const START_GIB=15;
 let held:string|undefined|null=null,short:number|null=null;
 while(!stopped()){
  try{
   const tracking=devHead();
   const statusFile=`${root}/full-loop-status.json`;
   if(fs.existsSync(statusFile)&&JSON.parse(fs.readFileSync(statusFile,'utf8')).head!==tracking.head)fs.rmSync(statusFile);
   const {phase}=JSON.parse(git('show',`${tracking.head}:tools/rv-gate/phase.json`)) as {phase:string};
   if(!['prelaunch','launch'].includes(phase))throw Error('Invalid gate phase');
   const mode=phase==='prelaunch'?'affected':'full';
   const hold=headHold(tracking.head,mode,tracking.base);
   if(hold){
    if(held!==hold.last){held=hold.last;const note=`\nFull loop holds ${tracking.head}: ${hold.reason}; ${hold.due?`the next run starts when dev moves or after ${hold.due}`:'the next run starts when dev moves'}.\n`;fs.appendFileSync(`${root}/ledger.md`,note);console.log(note.trim());}
    await pause(60000);continue;
   }
   held=null;
   await drainReleaseQueue();
   if(stopped())break;
   const current=devHead();
   if(current.head!==tracking.head||current.base!==tracking.base)continue;
   const free=Number(execFileSync('df',['-g','/'],{encoding:'utf8'}).trim().split('\n')[1].trim().split(/\s+/)[3]);
   if(!Number.isFinite(free))throw Error('Cannot read disk headroom');
   // Runs write evidence and rebuild missing outputs in the shared target. Refuse admission below
   // the shim's 15 GiB floor; admission above it does not establish the run's peak disk or memory fit.
   if(free<START_GIB){if(short!==free){short=free;console.log(`Waiting for ${START_GIB} GiB free to start a full: ${free} GiB free`);}await pause(30000);continue;}
   short=null;
   let memoryFreePercent=null;
   try{memoryFreePercent=Number(/System-wide memory free percentage:\s*(\d+)%/.exec(execFileSync('/usr/bin/memory_pressure',['-Q'],{encoding:'utf8',timeout:5000}))?.[1]);if(!Number.isFinite(memoryFreePercent))memoryFreePercent=null;}catch{}
   const parallel=Math.min(fullBudget(free,memoryFreePercent),2),time=new Date().toISOString().replace(/[-:]/g,'').replace(/\..*/,''),log=`${root}/logs/full-loop-${time}-${tracking.head.slice(0,10)}.log`;
   const stream=fs.createWriteStream(log),active:ActiveRun={state:'running',head:tracking.head,mode,parallel,started:new Date().toISOString(),log,controllerPid:process.pid,resourceAdmission:{freeGiB:free,memoryFreePercent,policy:'launch-week-cap2'}};
   let output='';const judgmentState:JudgmentState={};
   const acceptance=perfAcceptance(tracking.head);
   const environment={...process.env};delete environment.RV_PERF_ACCEPTED_GROWTH;
   if(acceptance){environment.RV_PERF_ACCEPTED_GROWTH=JSON.stringify(acceptance);active.baselineAcceptance=acceptance.id;console.log('Next full accepts only measured sizes/memory: '+JSON.stringify(acceptance));}
   const child=spawn(process.execPath,[`${launcher}/tools/rv-gate/rv-gate.mjs`,mode,tracking.head,'--parallel',String(parallel)],{cwd:launcher,env:environment,stdio:['ignore','pipe','pipe']});
   active.childPid=child.pid;save(activeFile,active);console.log(`Starting current ${mode} ${tracking.head}, parallel ${parallel}, ${free}GiB free, ${memoryFreePercent??'unknown'}% memory free (no performance acceptance)`);
   const receive=(chunk: string|Uint8Array<ArrayBufferLike>)=>{
    stream.write(chunk,()=>{try{considerJudgment(active,judgmentState);}catch(error){notify(LEAD,`Judgment error at ${active.head}: ${(error as Error).message}`);}});process.stdout.write(chunk);output+=chunk.toString();
    const receipt=/receipt ((?:full|affected)-[A-Za-z0-9-]+)/.exec(output)?.[1];if(receipt&&!active.receipt){active.receipt=receipt;save(activeFile,active);}
   };
   child.stdout.on('data',receive);child.stderr.on('data',receive);
   const code=await new Promise<number|null>((resolve,reject)=>{child.once('error',reject);child.once('close',resolve);});await new Promise<void>((resolve,reject)=>{stream.once('error',reject);stream.end(resolve);});considerJudgment(active,judgmentState);
   active.state='completed';active.exitCode=code;save(activeFile,active);const event=outcome(active,code);if(event.result!=='no receipt')lastInfra=null;promote(active,event);
   if(event.result==='no receipt')await pause(30000);
  }catch(error){const note=`\nFull loop infrastructure error: ${(error as Error).message}. No check waived; retry after30s.\n`;if(note!==lastInfra){notify(LEAD,note);lastInfra=note;}console.log(note);await pause(30000);}
 }
 console.log('Full loop stopped before starting another run; active adopted run was not killed.');
}finally{release();}
