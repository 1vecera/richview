// Derive the live installer test from the actual public seed, retaining its functional checks.
import {readFileSync,writeFileSync} from 'node:fs';
let s=readFileSync(process.argv[2],'utf8');
const base='https://github.com/1vecera/richview/releases/';
for(const [from,to] of [[base+'latest/download/install.ps1',base+'download/'+process.argv[4]+'/install.ps1'],['irm http://127.0.0.1:8765/install.ps1 | iex','irm '+base+'latest/download/install.ps1 | iex']]){
 if(s.split(from).length!==2)throw Error('Published installer test requires review for changed seed');s=s.replace(from,to);
}
writeFileSync(process.argv[3],s,{flag:'wx'});
