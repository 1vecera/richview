import {createHash} from 'node:crypto';
import {cpSync,existsSync,lstatSync,mkdtempSync,readFileSync,readdirSync,renameSync,rmSync} from 'node:fs';
import {basename,dirname,join} from 'node:path';

const hash = (bytes: Buffer | string) => createHash('sha256').update(bytes).digest('hex');
function inventory(dir: string) {
  const items: [string, string][] = [];
  function visit(relative: string) {
    const path = join(dir,relative);
    const stat = lstatSync(path);
    if (stat.isDirectory()) {
      items.push([relative,'directory']);
      for(const entry of readdirSync(path).sort()) visit(relative ? relative+'/'+entry : entry);
    } else if (stat.isFile()) {
      const bytes = readFileSync(path), digest = hash(bytes);
      if(relative.startsWith('assets/')){
        if(relative !== 'assets/'+digest+'.png')throw Error('Capture filename and bytes disagree: '+relative);
        if(bytes.subarray(0,8).toString('hex')!=='89504e470d0a1a0a')throw Error('Invalid comparison PNG: '+relative);
      }
      items.push([relative,digest]);
    } else throw Error('Unexpected link or special file in gate packet: '+path);
  }
  visit('');
  const paths=new Set(items.map(([name])=>name));
  if(!paths.has('assets')||!paths.has('entries.json'))throw Error('Incomplete gate report directory.');
  const html=readFileSync(join(dir,'index.html'),'utf8');
  for(const [tag] of html.matchAll(/<img\b[^>]*>/gi)) {
    const src=tag.match(/\bsrc=(["'])(.*?)\1/i)?.[2];
    if(!/^assets\/[a-f0-9]{64}\.png$/ .test(src ?? '')||!paths.has(src!))throw Error('Missing or invalid gate comparison image: '+src);
  }
  return hash(JSON.stringify(items));
}
const [action,index,here]=process.argv.slice(2);
if(basename(index)!=='index.html')throw Error('Expected the gate index.html file.');
const source=dirname(index), digest=inventory(source);
if(action==='verify'){
  if(basename(source)!=='gate-'+digest)throw Error('Retained packet directory changed.');
}else if(action==='retain'){
  const dest=join(here,'gate-'+digest);
  if(!existsSync(dest)){
    const tmp=mkdtempSync(join(here,'.packet-'));
    try{
      cpSync(source,tmp,{recursive:true});
      if(inventory(tmp)!==digest)throw Error('Gate packet changed during retention.');
      renameSync(tmp,dest);
    }finally{if(existsSync(tmp))rmSync(tmp,{recursive:true,force:true});}
  }
  if(inventory(dest)!==digest)throw Error('Previously retained gate packet changed.');
  console.log(join(dest,'index.html'));
}else throw Error('Use retain or verify.');
