// One editable plate per theme, viewport and interaction state, with the same fixed cases.
import {mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import {join, resolve} from 'node:path';
const dir=resolve(process.argv[2]);
const manifest=JSON.parse(readFileSync(join(dir,'manifest.json'),'utf8'));
const out=join(dir,'figma'); mkdirSync(out,{recursive:true});
const plates=[];
for(const capture of manifest.captures.filter(c=>c.case==='standard')) for(const state of ['rest','tooltip']) {
  let y=64, width=0, content='';
  const charts=[...capture.charts,
    ...manifest.captures.find(c=>c.case==='flagship'&&c.theme===capture.theme&&c.width===capture.width).charts.filter((_,i)=>[2,12,16].includes(i)),
    ...manifest.captures.find(c=>c.case==='bar-study-01'&&c.theme===capture.theme&&c.width===capture.width).charts];
  for(let i=0;i<charts.length;i++) {
    const chart=charts[i]; const svgPath=state==='rest'?chart.svg:chart.tooltip.replace(/\.png$/,'.svg');
    let svg=readFileSync(join(dir,svgPath),'utf8');
    const head=svg.slice(0,svg.indexOf('>')+1);
    const w=Number(head.match(/width="([\d.]+)"/)[1]), h=Number(head.match(/height="([\d.]+)"/)[1]);
    width=Math.max(width,w);
    // Each nested SVG owns its clip IDs; prefix fragment references before combining trees.
    svg=svg.replace(/id="([^"]+)"/g,(_,id)=>`id="case-${i}-${id}"`).replace(/url\(#([^)]+)\)/g,(_,id)=>`url(#case-${i}-${id})`);
    content+=`<g transform="translate(16 ${y})">${svg}</g>`; y+=h+32;
  }
  const name=`${manifest.phase}-${capture.theme}-${capture.width}-${state}`;
  const file=join(out,`${name}.svg`);
  const heading=width<480?`<tspan x="16" dy="0">${name} —</tspan><tspan x="16" dy="23">fixed review cases</tspan>`:`${name} — fixed review cases`;
  writeFileSync(file,`<svg xmlns="http://www.w3.org/2000/svg" width="${width+32}" height="${y}" viewBox="0 0 ${width+32} ${y}"><rect width="100%" height="100%" fill="#e8e8e8"/><text x="16" y="30" font-family="Space Grotesk" font-size="18">${heading}</text>${content}</svg>`);
  plates.push({name,file,width:width+32,height:y,charts:charts.length});
}
writeFileSync(join(out,'plates.json'),JSON.stringify(plates,null,2)+'\n');
console.log(`${plates.length} editable plates`);
