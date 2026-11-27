import {execFileSync,spawnSync} from 'node:child_process';
import {mkdirSync,rmdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {hostname} from 'node:os';
import {join} from 'node:path';
import {pathToFileURL,fileURLToPath} from 'node:url';
import {history,priorFor,reconcile,sole,verifyOldHome,confirmRestored} from './worker-history.mts';
const [action,root,,wt,,sha,id,hash,expected,here,candidate,words,authorizedFrom]=process.argv.slice(2);
const prior=priorFor(sha);
if(expected!==prior||!/^candidate\/[1-9]\d*$/.test(candidate))throw Error('Invalid reviewed Worker binding.');
const {gateHome,acquireLock}=await import(pathToFileURL(join(root,'tools/rv-gate/lib/store.mjs')).href) as typeof import('../../rv-gate/lib/store.mts');
const wr=join(wt,'node_modules/.bin/wrangler'),cfg=join(wt,'wrangler.toml'),binding={sha,id,candidate};
const lock=join(gateHome(),'deploy-site.lock'),owner=join(lock,'owner.json');
mkdirSync(lock);let unlock,owned=false;
try{
 writeFileSync(owner,JSON.stringify({pid:process.pid,host:hostname(),candidate}),{flag:'wx'});owned=true;
 unlock=acquireLock('merge-prod');
 if(action==='deploy'){
  if(sole(history(wr,cfg)[0])!==prior)throw Error('Unexpected current Worker.');
  execFileSync(process.execPath,[fileURLToPath(new URL('./deploy-kept-reviewed.mts',import.meta.url)),'--candidate',candidate,'--receipt',id,'--manifest-sha256',hash,'--go',words],{stdio:'inherit',env:{...process.env,RV_SITE_JOURNAL:here}});
  const {current,from}=reconcile(wr,cfg,here,binding);
  if(!from||current!==from)throw Error('Cannot attribute deployed Worker to this write.');
 }else if(['inspect','rollback'].includes(action)){
  const result=reconcile(wr,cfg,here,binding);
  const {current,from,pending,attempt}=result;
  if(pending){
   console.error('UNRESOLVED: '+attempt+' has no deployment in history yet. Observed '+current+' at 100%. Keep the attempt marker and run recover-worker-history again before closing recovery or retrying.');
   verifyOldHome(prior);
   throw Error('Marked Worker request remains unresolved.');
  }
  if(current===prior){confirmRestored(here,result,prior);console.log('Reviewed Worker is already live at 100%; retained homepage bytes match.');}
  else if(action==='inspect'){console.log('Proven recovery from '+from+' to '+prior);}
  else{
   if(!from||authorizedFrom!==from||current!==from)throw Error('Authorized recovery version differs from active attempt. Inspect history again.');
   if(!process.stdin.isTTY||!process.stdout.isTTY)throw Error('Worker rollback needs an attended terminal for Wrangler confirmation.');
   const result=spawnSync(wr,['rollback',prior,'--config',cfg,'--message','Restore reviewed Worker '+prior,'--yes'],{stdio:'inherit'});
   if(result.error||result.status!==0)throw Error('Rollback response incomplete. Inspect under locks before retry.');
   const verified=reconcile(wr,cfg,here,binding);
   if(verified.current!==prior||verified.from!==from)throw Error('Restored Worker history or 100% traffic mismatch.');
   confirmRestored(here,verified,prior);
  }
 }else throw Error('Unknown Worker operation.');
}finally{unlock?.();if(owned)unlinkSync(owner);rmdirSync(lock);}
