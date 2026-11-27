// Real-process verification tooling. Every write stays in a temporary loopback store.
import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {createHash,randomUUID} from 'node:crypto';
import {mkdirSync,mkdtempSync,readFileSync,writeFileSync} from 'node:fs';
import http from 'node:http';
import type { AddressInfo } from 'node:net';
import {resolve,join} from 'node:path';

type Reply<Result> = { id: number; result: Result };
type CommandReply = Record<string, unknown>;
type ToolSchema = { name: string; inputSchema: { properties: Record<string, { enum: string[] }> } };
type NativeReplies = [
 Reply<{ protocolVersion: string }>, Reply<{ tools: ToolSchema[] }>, Reply<{ resources: unknown[] }>,
 Reply<unknown>, Reply<{ isError: boolean }>, Reply<{ isError: boolean }>,
];
type AssetReply = Reply<{
 isError: boolean;
 structuredContent: { checked: { sha256?: string; diagnostics: { code: string }[] }[]; code?: string };
}>;

const root=resolve(new URL('..',import.meta.url).pathname);
mkdirSync(join(root,'tmp/verification'),{recursive:true});
const dir=mkdtempSync(join(root,'tmp/verification/native-'));
const cli=join(root,'target/release/richview');
const token=randomUUID();
const env: NodeJS.ProcessEnv={...process.env,XDG_CONFIG_HOME:join(dir,'config'),RICHVIEW_TOKEN:token};delete env.RICHVIEW_PUBLIC_URL;
const run=(...args: (string | number)[])=>{const result=spawnSync(cli,args.map(String),{cwd:root,encoding:'utf8',env,timeout:30000});assert.equal(result.status,0,result.stderr||result.stdout);return result.stdout;};
const source='---\ntitle: Rust lifecycle\ndescription: Exact source survives native boundaries.\nversion: 0.0.1\nslug: rust-lifecycle\nstyles: [sleek]\n---\n\n# Source matters\n\n```chart\ntype: bar\ntitle: B exceeds A\ndata: {inline: [{category: A, value: 12}, {category: B, value: 20}]}\nencoding: {x: category, y: value}\n```\n';
const report=join(dir,'report');mkdirSync(report);const file=join(report,'report.rmd');writeFileSync(file,source);
const evidence: { build: string; commands: Record<string, CommandReply>; mcp: CommandReply; server: CommandReply }={build:'release',commands:{},mcp:{},server:{}};
for(const command of ['generate','validate','check']){
 const value: CommandReply=JSON.parse(run(command,file,'--format','json'));
 evidence.commands[command]=Object.fromEntries(Object.entries(value).filter(([key])=>['ok','blocks','status','interactive'].includes(key)));
}
assert.ok(readFileSync(join(report,'Rendered/report.html'),'utf8').startsWith('<!doctype html>'));
assert.equal(evidence.commands.generate.interactive,true);
const requests=[
 {id:1,method:'initialize',params:{protocolVersion:'2025-06-18',capabilities:{},clientInfo:{name:'local-test',version:'1'}}},
 {method:'notifications/initialized'},
 {id:2,method:'tools/list'},
 {id:3,method:'resources/list'},
 {id:4,method:'resources/read',params:{uri:'richview://spec/rmd'}},
 {id:5,method:'tools/call',params:{name:'richview_render',arguments:{inputPath:file,outputName:'mcp.html'}}},
 {id:6,method:'tools/call',params:{name:'richview_check',arguments:{inputPath:file}}},
];
const mcp=spawnSync(cli,['mcp'],{cwd:root,encoding:'utf8',env,timeout:30000,input:requests.map(request=>JSON.stringify({jsonrpc:'2.0',...request})+'\n').join('')});
assert.equal(mcp.status,0,mcp.stderr);
const replies=mcp.stdout.trim().split('\n').map(line=>JSON.parse(line)) as NativeReplies;
assert.deepEqual(replies.map(r=>r.id),[1,2,3,4,5,6]);
assert.equal(replies[1].result.tools.length,9);assert.equal(replies[2].result.resources.length,2);
const renderSchema=replies[1].result.tools.find(tool=>tool.name==='richview_render')!.inputSchema.properties;
assert.deepEqual(renderSchema.output.enum,['report','pdf']);assert.deepEqual(renderSchema.page.enum,['a4','letter']);
assert.equal(replies[4].result.isError,false);assert.equal(replies[5].result.isError,false);
assert.ok(readFileSync(join(report,'Rendered/mcp.html'),'utf8').includes('Source matters'));
evidence.mcp={protocolVersion:replies[0].result.protocolVersion,tool_count:7,resource_count:2,render_file_written:true,check_success:true,stdout_protocol_only:true};
// Asset tools: verify a loopback object by HEAD and content hash; put reports a missing store profile.
const csv=Buffer.from('month,orders\n2026-07,120\n2026-08,150\n');
const host=http.createServer((request,response)=>{response.writeHead(200,{'content-type':'text/csv','content-length':csv.length});response.end(request.method==='HEAD'?undefined:csv);});
await new Promise<void>(accept=>host.listen(0,'127.0.0.1',accept));
const address = host.address() as AddressInfo;
const asset={url:`http://127.0.0.1:${address.port}/data/orders.csv`,bytes:csv.length,sha256:createHash('sha256').update(csv).digest('hex'),type:'text/csv',visibility:'public'};
const assetCalls=[{name:'richview_asset_verify',arguments:{asset,hash:true}},{name:'richview_asset_verify',arguments:{asset:{...asset,url:asset.url+'?X-Amz-Signature=0'}}},{name:'richview_asset_put',arguments:{path:file,store:'missing'}}];
const assetReplies=await new Promise<AssetReply[]>((accept,reject)=>{const child=spawn(cli,['mcp'],{cwd:root,env:{...env,RICHVIEW_STORES:join(dir,'stores.yaml')},stdio:['pipe','pipe','inherit']});let out='';child.stdout.on('data',chunk=>{out+=chunk;});child.on('error',reject);child.on('close',()=>accept(out.trim().split('\n').map(line=>JSON.parse(line))));child.stdin.end(assetCalls.map((params,id)=>JSON.stringify({jsonrpc:'2.0',id,method:'tools/call',params})+'\n').join(''));});
host.close();
const [verified,presigned,missing]=assetReplies.map(reply=>reply.result);
assert.equal(verified.isError,false);assert.equal(verified.structuredContent.checked[0].sha256,'match');
assert.equal(presigned.isError,true);assert.equal(presigned.structuredContent.checked[0].diagnostics[0].code,'asset.presigned');
assert.equal(missing.isError,true);assert.equal(missing.structuredContent.code,'asset.invalid');
evidence.mcp.asset_tools={verify_hash_match:true,presigned_rejected:true,missing_store_reported:true};
const server=spawn(join(root,'target/release/richview'),['server','--bind','127.0.0.1','--port','0','--default-visibility','public','--data-dir',join(dir,'store')],{cwd:root,env,stdio:['ignore','pipe','pipe']});
try{
 const base=await new Promise<string>((accept,reject)=>{let log='';const timeout=setTimeout(()=>reject(new Error('Native server timeout')),10000);server.once('error',reject);server.once('exit',code=>reject(new Error(`Server stopped ${code}`)));for(const stream of [server.stdout,server.stderr])stream.on('data',chunk=>{log+=chunk;const match=log.match(/RichView at (http:\/\/127\.0\.0\.1:\d+)/);if(match){clearTimeout(timeout);accept(match[1]);}});});
 assert.equal((await (await fetch(base+'/health')).json()).ok,true);
 env.RICHVIEW_SERVER=base;
 const unauthorized=await fetch(base+'/api/reports',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({slug:'rust-lifecycle',source})});assert.equal(unauthorized.status,401);
 const first: { created: boolean }=JSON.parse(run('publish',file,'--server',base,'--token',token,'--format','json'));assert.equal(first.created,true);
 const typo=spawnSync(cli,['publish',file,'--server',base,'--token',token,'--slug','rust-lifecyle','--format','json'],{cwd:root,encoding:'utf8',env,timeout:30000});assert.notEqual(typo.status,0);assert.match(typo.stdout,/publish\.new-slug/);
 writeFileSync(file,source.replace('Source matters','A later buffer must not overwrite the published version'));
 const second=spawnSync(cli,['publish',file,'--server',base,'--token',token,'--format','json'],{cwd:root,encoding:'utf8',env,timeout:30000});assert.notEqual(second.status,0,'a changed source under a published version is refused');assert.match(second.stdout,/publish\.version-exists/);
 assert.equal(await (await fetch(base+'/raw/rust-lifecycle/v/0.0.1')).text(),source);
 const served=await (await fetch(base+'/r/rust-lifecycle')).text();assert.match(served,/<h2 id="source-matters">Source matters<\/h2>/);assert.ok(!served.includes('A later buffer'));
 const regenerate=await fetch(base+'/api/reports/rust-lifecycle/v/0.0.1/regenerate',{method:'POST',headers:{authorization:'Bearer '+token}});assert.equal(regenerate.status,200);
 assert.equal((await (await fetch(base+'/api/reports',{headers:{authorization:'Bearer '+token}})).json()).length,1);
 evidence.server={loopback_only:true,auth_required:true,publish_created:true,republish_immutable:true,raw_source_identical:true,rendered_html_served:true,regeneration_success:true,listing_success:true};
 writeFileSync(join(dir,'native.json'),JSON.stringify(evidence,null,2)+'\n');console.log(JSON.stringify({...evidence,evidence_path:join(dir,'native.json')},null,2));
}finally{const exited=new Promise(accept=>server.once('exit',accept));server.kill('SIGINT');await Promise.race([exited,new Promise(accept=>setTimeout(accept,3000))]);if(server.exitCode===null)server.kill('SIGKILL');}
