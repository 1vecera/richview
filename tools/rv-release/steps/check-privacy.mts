// Match the release-29 reopen guard (the approved provider section is blob 01e3d6aa, unchanged since) and check the independently approved cookie sentence.
import {execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
const source=readFileSync(process.argv[2],'utf8');
const heading='<h2>Signing in with GitHub, Microsoft or Google</h2>';
function section(html: string,title: string){
 const hits=[...html.replace(/<!--[\s\S]*?-->/g,'').matchAll(/<section\b[^>]*>[\s\S]*?<\/section>/gi)].map(m=>m[0].replace(/\s+/g,' ').trim()).filter(s=>s.includes(title));
 if(hits.length!==1)throw Error('Expected exactly one section headed '+title);
 return hits[0];
}
const approved=execFileSync('git',['cat-file','blob','01e3d6aadceb17c83d0ca867569801d68db1f2e0'],{encoding:'utf8'});
if(section(source,heading)!==section(approved,heading))throw Error('Provider section differs from the release-29 approval.');
const cookie='When you open a report from the app while signed in, the report address sets a second cookie for that report, for one hour, so the report can load; it is not used for tracking.';
if(!section(source,'<h2>Cookies and analytics</h2>').includes(cookie))throw Error('Approved report-cookie sentence is absent.');
console.log('Provider section and one-hour report cookie passed.');
