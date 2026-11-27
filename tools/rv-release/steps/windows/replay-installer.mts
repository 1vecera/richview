// Replay the retained walk's HTTP framing and command on macOS, without running a Windows install.
import {createServer} from 'node:http';
import {spawn} from 'node:child_process';
import {existsSync,mkdtempSync,readFileSync,rmSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {once} from 'node:events';

const [executable,installer]=process.argv.slice(2);
if(process.platform!=='darwin'||!executable||!installer)throw Error('Usage on Claw Mini: node replay-installer.mjs /absolute/path/to/pwsh retained-install.ps1');
const shell=resolve(executable), file=resolve(installer), bytes=readFileSync(file);
if(!existsSync(shell))throw Error('Local PowerShell executable is missing.');
const scratch=mkdtempSync(join(process.cwd(),'.walk-parser-'));
const server=createServer((req,res)=>{
  if(req.url!=='/install.ps1'){res.writeHead(404);res.end();return;}
  res.writeHead(200,{'Content-Length':bytes.length});
  res.end(bytes);
});
function invoke(code: string){
  const child=spawn(shell,['-NoProfile','-ExecutionPolicy','Bypass','-Command',code],{
    cwd:scratch,env:{PATH:process.env.PATH,HOME:scratch,USERPROFILE:scratch,TMPDIR:scratch,POWERSHELL_TELEMETRY_OPTOUT:'1',INSTALLER:file},stdio:['ignore','inherit','inherit']});
  return once(child,'exit').then(([rc])=>rc as number | null);
}
try{
  server.listen(8765,'127.0.0.1');await once(server,'listening');
  const parser=await invoke(`$ErrorActionPreference='Stop'; $t=$null; $e=$null; $s=[IO.File]::ReadAllText($env:INSTALLER); $null=[Management.Automation.Language.Parser]::ParseInput($s,[ref]$t,[ref]$e); "Full file: $($s.Length) characters, $($e.Count) parse errors"; if($e.Count){exit 1}; $null=[Management.Automation.Language.Parser]::ParseInput('& {',[ref]$t,[ref]$e); "Isolated fragment: $($e.ErrorId)"; if($e.ErrorId -notcontains 'MissingEndCurlyBrace'){exit 1}`);
  if(parser!==0)throw Error('PowerShell parser control failed.');
  const result=await invoke('irm http://127.0.0.1:8765/install.ps1 | iex');
  console.log('Retained one-liner process exit '+result+'. Inspect its error: reaching the Windows environment checks proves the full input parsed on this runtime.');
}finally{
  if(server.listening)await new Promise<void>(done=>server.close(() => done()));
  rmSync(scratch,{recursive:true,force:true});
}
