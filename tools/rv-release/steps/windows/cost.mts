// Charge every recorded instance of this release across abandoned SHAs. Fail closed on unsettled attempts.
// A release starts at 0 USD: the previous release's windows-* folders do not count against this candidate's ceiling.
// Windows 'combined' and 'site' scopes both need the board ask named in the release descriptor (decision.mjs).
import {existsSync, readdirSync,readFileSync} from 'node:fs';
import {join,dirname,basename,resolve} from 'node:path';
import {authorization,latestAnswer,permits} from './decision.mts';
const [dir,phase='settled',operation,selection]=process.argv.slice(2),root=dirname(resolve(dir)),here=join(root,'go-record-'+basename(dir).slice(8,16));
const rate=(0.7754+0.005+80*0.0952/730)/60;
const duration: Record<string, number> = {combined:95,site:73,'private-walk':22,bonus:73,public:22,settled:0};
const minutes=duration[phase]; if(minutes===undefined)throw Error('Unknown budget phase');
const read=(f: string)=>readFileSync(f,'utf8').trim();
let actual=0;
const other=join(root,'additional-aws-usd.txt');
if(existsSync(other)){const raw=read(other);if(!/^\d+(\.\d+)?$/.test(raw))throw Error('Expected nonnegative additional AWS charges');actual+=Number(raw);}
for(const name of readdirSync(root).filter(x=>/^windows-[a-f0-9]{40}$/.test(x))){
 for(const lane of ['aws-build','bonus-build','private/walk','private/walk-2','public/walk']){
 const p=join(root,name,lane);
 if(existsSync(join(p,'reconciled-charge.json'))){
  const x=JSON.parse(read(join(p,'reconciled-charge.json'))) as {token: string; id: string; state: string; upperUsd: number};
  if(x.token!==read(join(p,'launch-token'))||!/^i-[a-f0-9]+$/.test(x.id)||x.state!=='terminated'||!Number.isFinite(x.upperUsd)||x.upperUsd<=0||(existsSync(join(p,'instance-id'))&&read(join(p,'instance-id'))!==x.id))throw Error('Invalid EC2 reconciliation at '+p);
  actual+=x.upperUsd;continue;
 }
 if(!existsSync(join(p,'instance-id'))){
   if(existsSync(join(p,'launch-token')))throw Error('Unreconciled EC2 launch at '+p+'; look up the client token, termination and actual charge before another attempt.');
   if(existsSync(p)&&(!existsSync(join(p,'prelaunch-sweep.log'))||!/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ zero live tagged instances, volumes, security groups, snapshots or keys$/.test(read(join(p,'prelaunch-sweep.log')))))throw Error('Pre-launch resource sweep is incomplete: '+p);
   continue;
 }
 if(!/^i-[a-f0-9]+$/.test(read(join(p,'instance-id'))))throw Error('Invalid EC2 instance identity at '+p);
 if(read(join(p,'state-after.txt'))!=='terminated')throw Error('Unsettled Windows instance');
 const cost=lane.includes('/walk')?(JSON.parse(read(join(p,'manifest.json'))) as {costUsd: number}).costUsd:Number(read(join(p,'cost.txt')).match(/([0-9]+\.[0-9]+) USD$/)?.[1]);
 if(!Number.isFinite(cost)||cost<=0)throw Error('Missing billed estimate');actual+=cost;
 }}
let limit=9, scope='stop';
const sha=basename(dir).slice(8);
if(operation==='quote'){
 const decision=authorization(selection,phase,sha,latestAnswer());limit=decision.limit;scope=decision.scope;
}else if(existsSync(join(here,'windows-ceiling.txt'))){
 const record=JSON.parse(read(join(here,'windows-ceiling.txt'))) as ReturnType<typeof authorization>;
 const expected=authorization(record.route,record.scope,sha,latestAnswer());
 if(JSON.stringify(record)!==JSON.stringify(expected)||read(join(here,'windows-budget-go.txt'))!==JSON.stringify(expected.answer))throw Error('Windows ceiling, date, SHA or board selection mismatch');
 limit=expected.limit;scope=expected.scope;
}
const reserve=phase==='private-walk'&&process.env.D_WINDOWS_MODE==='combined'?22:0;
const projected=actual+(minutes+reserve)*rate;console.log(JSON.stringify({sha,actual:+actual.toFixed(4),minutes,reservedPublicMinutes:reserve,projected:+projected.toFixed(4),limit,phase,scope}));
if(!permits(scope,phase)||projected>limit)throw Error('Phase outside the approved ceiling or projection exceeds the ceiling; no allocation or launch.');
