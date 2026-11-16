// Host reason (ARC-03): run the installed VSIX in a downloaded VS Code with isolated test state.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, copyFileSync, cpSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { downloadAndUnzipVSCode, resolveCliArgsFromVSCodeExecutablePath, runTests } from '@vscode/test-electron';
import { gateHome } from '../tools/rv-gate/lib/store.mjs';
import { waitForBenchmark } from '../tools/rv-gate/lib/quiet.mjs';

const {values}=parseArgs({options:{out:{type:'string'},vsix:{type:'string'}}});
assert.equal(process.platform,'darwin','The native VS Code full step runs on Claw Mini macOS; Linux GUI checks require offscreen isolation');
await waitForBenchmark();
const root=process.cwd();
const artifact=resolve(values.out??join(root,'tmp',`vscode-${process.pid}`));
mkdirSync(artifact,{recursive:true});
rmSync(join(artifact,'vscode.json'),{force:true});
// macOS Unix-domain IPC paths must stay under103bytes, including VS Code's socket filename.
const workspace=mkdtempSync(join(artifact,'workspace-')),profile=mkdtempSync(join(tmpdir(),'rv-vsc-')),extensions=join(artifact,'extensions'),driver=join(artifact,'driver');
try {
for(const dir of [workspace,join(profile,'User'),extensions,driver])mkdirSync(dir,{recursive:true});
for(const file of ['report.rmd','report.rstyle.yaml'])copyFileSync(join(root,'examples/authoring',file),join(workspace,file));
writeFileSync(join(profile,'User/settings.json'),JSON.stringify({'telemetry.telemetryLevel':'off','update.mode':'none','extensions.autoUpdate':false,'extensions.autoCheckUpdates':false,'workbench.enableExperiments':false,'workbench.startupEditor':'none','security.workspace.trust.enabled':false,'git.enabled':false,'editor.minimap.enabled':false}));
writeFileSync(join(driver,'package.json'),JSON.stringify({name:'richview-verification-driver',publisher:'local-test',version:'0.0.1',engines:{vscode:'^1.90.0'}}));
const version='1.136.1';
const cache=join(gateHome(),'cache/vscode');
mkdirSync(cache,{recursive:true});
const executable=await downloadAndUnzipVSCode({version,cachePath:cache});
const [cli,...cliArgs]=resolveCliArgsFromVSCodeExecutablePath(executable,{reuseMachineInstall:true});
const isolated=[`--user-data-dir=${profile}`,`--extensions-dir=${extensions}`];
const manifest=JSON.parse(readFileSync(join(root,'packages/vscode/package.json')));
const target=`${process.platform}-${process.arch}`;
const vsix=resolve(values.vsix??join(root,`packages/vscode/richview-vscode-${target}-${manifest.version}.vsix`));
assert.ok(existsSync(vsix),`Package the current build before the VS Code step: ${vsix}`);
const env={...process.env,VSCODE_IPC_HOOK_CLI:undefined,VSCODE_PORTABLE:undefined,ELECTRON_RUN_AS_NODE:undefined};
writeFileSync(join(artifact,'runner.json'),JSON.stringify({version,executable,profile,extensions,workspace,vsix},null,2));
const installed=spawnSync(cli,[...cliArgs,...isolated,'--install-extension',vsix,'--force'],{encoding:'utf8',timeout:120000,env});
process.stdout.write(installed.stdout??'');process.stderr.write(installed.stderr??'');
assert.equal(installed.status,0,'Install the actual VSIX into the isolated extensions directory');
const port=await new Promise((accept,reject)=>{const server=createServer();server.once('error',reject);server.listen(0,'127.0.0.1',()=>{const port=server.address().port;server.close(error=>error?reject(error):accept(port));});});
let timedOut=false,forceClose;
const interrupt=()=>process.kill(process.pid,'SIGINT');
const deadline=setTimeout(()=>{timedOut=true;interrupt();forceClose=setTimeout(interrupt,5000);},120000);
const terminate=()=>{timedOut=true;interrupt();};
process.on('SIGTERM',terminate);
try {
  await runTests({vscodeExecutablePath:executable,extensionDevelopmentPath:driver,extensionTestsPath:join(root,'scripts/verify-vscode.cjs'),launchArgs:[workspace,...isolated,`--remote-debugging-port=${port}`,'--disable-telemetry','--use-inmemory-secretstorage','--password-store=basic'],extensionTestsEnv:{VSCODE_IPC_HOOK_CLI:undefined,VSCODE_PORTABLE:undefined,ELECTRON_RUN_AS_NODE:undefined,RICHVIEW_VSCODE_ARTIFACT_DIR:artifact,RICHVIEW_TEST_EXTENSIONS:extensions,RICHVIEW_TEST_CDP_PORT:String(port)}});
  assert.equal(timedOut,false,'VS Code behavior must finish within the existing120second deadline');
} finally {clearTimeout(deadline);clearTimeout(forceClose);process.removeListener('SIGTERM',terminate);}
assert.ok(existsSync(join(artifact,'vscode.json')),'The complete installed-extension behavioral evidence must exist');

} finally {
  if(existsSync(join(profile,"logs")))cpSync(join(profile,"logs"),join(artifact,"logs"),{recursive:true});
  rmSync(profile,{recursive:true,force:true});
}
