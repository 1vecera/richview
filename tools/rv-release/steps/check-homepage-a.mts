import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {parse, defaultTreeAdapter} from 'parse5';
import type {DefaultTreeAdapterMap} from 'parse5';

type Node = DefaultTreeAdapterMap['node'];
type Element = DefaultTreeAdapterMap['element'];

const [html,manifest]=process.argv.slice(2);
const bytes=readFileSync(html);
const expected=(JSON.parse(readFileSync(manifest,'utf8')) as {outputs?: Record<string,{sha256?: string}>}).outputs?.['checkout:dist/web/index.html']?.sha256;
if(createHash('sha256').update(bytes).digest('hex')!==expected)throw Error('Staged homepage differs from its reviewed build manifest.');

const attr=(node: Node,name: string)=> ('attrs' in node ? node.attrs : undefined)?.find(a=>a.name===name)?.value??'';
const hasClass=(node: Node,name: string)=>attr(node,'class').split(/\s+/).includes(name);
const children=(node: Node | undefined) => (node && "childNodes" in node ? node.childNodes : []).filter(defaultTreeAdapter.isElementNode);
function elements(node: Node): Element[]{
  return [...(defaultTreeAdapter.isElementNode(node) ? [node] : []),...children(node).flatMap(elements)];
}
function text(node: Node): string {return defaultTreeAdapter.isTextNode(node)?node.value:('childNodes' in node ? node.childNodes : []).map(text).join('');}
function one(nodes: Element[],predicate: (node: Element) => boolean,label: string){
  const matches=nodes.filter(predicate);
  if(matches.length!==1)throw Error('Expected one '+label+' in the staged variant-A homepage.');
  return matches[0];
}
function exact(node: Element | undefined,words: string){
  if(!node||text(node).replace(/\s+/g,' ').trim()!==words)throw Error('Variant-A homepage must say “'+words+'”.');
}

const all=elements(parse(bytes.toString('utf8')));
const box=one(all,n=>hasClass(n,'explain'),'explainer'), parts=children(box);
const problem=one(all,n=>attr(n,'id')==='problem','problem section');
const items=children(problem);
const heading=one(items,n=>n.tagName==='h2'&&attr(n,'id')==='h-problem','problem heading');
const proof=one(items,n=>n.tagName==='div'&&hasClass(n,'proof'),'proof cards');
const statement=one(items,n=>n.tagName==='p'&&hasClass(n,'statement'),'statement');
exact(statement,"There's a better way: RichView.");
if(items.indexOf(proof)!==items.indexOf(heading)+1 || items.indexOf(statement)<=items.indexOf(proof) || items.indexOf(box)<=items.indexOf(statement))throw Error('Expected the problem heading, proof cards, statement and explainer in that order.');
exact(one(parts,n=>attr(n,'id')==='h-what','heading'),'How does RichView work?');
exact(one(parts,n=>n.tagName==='p'&&hasClass(n,'explain__lead'),'lead sentence'),'RichView is a CLI that works entirely offline to turn a text specification into a beautiful, interactive HTML file with charts, images and more.');
const ecosystem=one(parts,n=>n.tagName==='p'&&hasClass(n,'explain__ecosystem'),'ecosystem sentence');
exact(ecosystem,'The broader ecosystem adds publishing, a VS Code extension, slides, brand styles and backgrounds.');
const flow=one(parts,n=>n.tagName==='ol'&&hasClass(n,'eco--workflow'),'workflow');
exact(children(children(flow)[0]).find(n=>n.tagName==='h4'),'Write a specification');
if(elements(ecosystem).some(n=>n.tagName==='a')||all.some(n=>
  attr(n,'id')==='h-home-slides'||hasClass(n,'home-slides')||
  (n.tagName==='a'&&(/\/monthly-review\//.test(attr(n,'href'))||attr(n,'href').startsWith('/docs/slides')))||
  (n.tagName==='img'&&attr(n,'src').startsWith('/landing/slides/'))
))throw Error('Homepage contains a slide link or block.');
