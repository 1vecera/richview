// Offline text inventory of the website and copy handed to authors.
import {execFileSync} from 'node:child_process';
import {readFileSync,readdirSync} from 'node:fs';
import {join} from 'node:path';
const [mode,target,base]=process.argv.slice(2); // base: the commit of the release now live, for the 'changed source' rows
const patterns: [string, RegExp][]=[
 ['version',/\b(?:v?3\.[01](?:\.\d+)?(?:-alpha\.\d+)?|alpha\.[34]|latest (?:version|release)|installed richview)\b/i],
 ['install',/releases\/latest|github\.com\/1vecera\/richview\/(?:tree|blob|releases)|install richview from|\/install\.(?:ps1|sh)\b/i],
 ['slides',/\bslides?\s*:|\/docs\/slides\b|monthly-review|\bslide deck\b|turn .* into slides|one slide per|september (?:business )?review/i],
 ['publish and identity',/richview\s+(?:login|logout|publish)\b|--new\b|browser (?:approval|sign.in)|sign in (?:using|with|through) (?:your browser|github)|cli token|device code|creates? (?:a |the )?(?:private |first )?report|first publish/i],
 ['data and tables',/parquet|heat:\s*(?:true|diverging)|filter:\s*(?:true|false)|heat (?:scale|shading)|table filter|filter box|filterable|filter (?:the |a )?(?:table|rows)/i],
 ['chart emphasis and annotations',/\bhighlight(?:\.mute)?\s*:|mut(?:e|ing) (?:other|the)|highlight (?:a |one |the )?(?:series|bar|point)|\banchor:\s*\{?|data.anchored|annotation.*(?:point|leader|label)|data (?:point|coordinate).*annotation/i],
 ['style and rendering',/\bbackground\s*:|\/docs\/backgrounds|paper grain|faint grid|syntax highlight|code highlight|SQL keywords|retired (?:setting|style)|\.rstyle\.yaml|style (?:override|setting)/i],
 ['hosted and limits',/5\s*(?:MB|MiB)|50\s*(?:MB|MiB)|publishing limit|sign.up capacity|account delet|semver|public link.*(?:source|markdown)|one.hour.*cookie/i],
 ['reader changes',/last frame|restart.*playback|end values|end labels|rule label|panel titles|keyboard focus|highlighted marks|filter.*accent/i],
];
const ext=/\.(?:html?|md|txt|rmd|ya?ml|json)$/i, roots=['assets','docs/public','docs/RMD-SPEC.md','docs/surface','skill','examples/openai-vs-anthropic','crates/richview-build/src','crates/richview-app/src/reference.rs','crates/richview-core/src/knowledge'];
const git=(...args: string[])=>execFileSync('git',args,{encoding:'utf8',maxBuffer:64<<20});
const clean=(s: string)=>s.replace(/[\t\r\n]+/g,' ').replace(/\s+/g,' ').trim();
console.log('surface\tlocation\tline\tcontext');
function scan(file: string,raw: string){
 const content=/\.html?$/i.test(file)?raw.replace(/<!--[\s\S]*?-->|<(script|style)\b[^>]*>[\s\S]*?<\/\1>/gi,m=>m.replace(/[^\n]/g,' ')):raw;
 const lines=content.split('\n');
 for(const [i,line] of lines.entries()){
  const html=/\.html?$/i.test(file);
  // Preserve link destinations and code text. Split adjacent tags even in minified HTML.
  const fragments=html?line.split(/(?=<)|(?<=>)/):[line];
  for(const [kind,pattern] of patterns){
   const found=fragments.find(f=>pattern.test(f))??(pattern.test(clean(line.replace(/<[^>]*>/g,' ')))?clean(line.replace(/<[^>]*>/g,' ')):null);
   if(!found)continue;
   const s=clean(found), pos=s.search(pattern);
   console.log([kind,file,i+1,s.slice(Math.max(0,pos-45),Math.max(0,pos)+160)].join('\t'));
  }
 }
}
if(mode==='source'){
 const sha=git('rev-parse','--verify',target+'^{commit}').trim();
 const files=git('ls-tree','-r','--name-only',sha,'--',...roots).trimEnd().split('\n');
 for(const f of files.filter(f=>ext.test(f)||f.endsWith('.rs')))scan(f,git('show',sha+':'+f));
 for(const f of git('diff','--name-only',git('rev-parse','--verify',base+'^{commit}').trim(),sha,'--',...roots).trim().split('\n').filter(f=>ext.test(f)||f.endsWith('.rs')))
   console.log(['changed source vs live release',f,'','Review the candidate diff, including claims without a keyword.'].join('\t'));
}else if(mode==='kept'){
 function walk(dir: string,prefix=''){for(const entry of readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name))){
   const f=prefix+entry.name, p=join(dir,entry.name);
   if(entry.isDirectory())walk(p,f+'/');else if(entry.isFile()&&ext.test(f))scan(f,readFileSync(p,'utf8'));
 }}
 walk(target);
}else throw Error('Use source <git commit> <live release commit> or kept <verified dist/web>.');
