// Host reason (ARC-03): exercise the shipping native-scroll setup without starting a browser.
import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import { readTestSource } from './test-source.mts';
const source=readTestSource(process.env.READOUT_SOURCE_UNDER_TEST||new URL('../../../scripts/verify-chart-readouts.ts',import.meta.url));
const start=source.indexOf('// With the selection cleared');
assert.ok(start>0);
const block=source.slice(start,source.indexOf('\n    }',start));
const check=source.slice(source.indexOf('const check'),source.indexOf('\n\n/** The readout'));
type ScrollEnd = (event: { target: object }) => void;
type ScrollWait = { fn: (arg: unknown) => boolean; arg: unknown; resolve: (value: boolean) => void; reject: (error: Error) => void; done?: boolean };
function scenario(){
 const listeners=new Set<ScrollEnd>(),waits:ScrollWait[]=[],cases:string[]=[],failures:string[]=[];let resets=0,disposed=0;
 const document={addEventListener:(name: string,fn: ScrollEnd)=>{assert.equal(name,'scrollend');listeners.add(fn);},removeEventListener:(_name: string,fn: ScrollEnd)=>listeners.delete(fn)};
 const context=vm.createContext({document,cases,failures,scrollY:0,at:'scatter @390',figure:{},assert,settle:async()=>{},state:async()=>({hidden:true,page:0}),scrollTo:()=>{resets++;context.scrollY=0;}});
 const sample=()=>{for(const w of waits)if(!w.done&&w.fn(w.arg)){w.done=true;w.resolve(true);}};
 const unwrap=(value: unknown)=>value && typeof value==='object' && 'remote' in value ? value.remote ?? value : value;
 context.page={
  keyboard:{press:async (key: string)=>{if(key==='PageDown'){context.scrollY=1;sample();}}},
  evaluate:async <T,>(fn: () => T)=>fn(),
  evaluateHandle:async <T,>(fn: () => T)=>({remote:fn(),evaluate:async function<R>(fn: (value: T) => R){return fn(this.remote);},dispose:async()=>{disposed++;}}),
  // The VM supplies the predicate and its serialized Playwright argument.
  waitForFunction:(fn: ScrollWait["fn"],arg: unknown,options: {timeout: number})=>{assert.equal(options.timeout,5000,'Keep the existing five-second bound');const actual=arg&&typeof arg==='object'?Object.fromEntries(Object.entries(arg).map(([key,value])=>[key,unwrap(value)])):arg;return new Promise<boolean>((resolve,reject)=>{waits.push({fn,arg:actual,resolve,reject});sample();});},
 };
 vm.runInContext(check+'\nglobalThis.run=async()=>{'+block+'};',context);
 const run: () => Promise<void> = context.run;
 return{run,resets:()=>resets,disposed:()=>disposed,listeners,waits,cases,failures,finish(target: object=document){context.scrollY=2;for(const fn of [...listeners])fn({target});sample();}};
}
const flush=async()=>{for(let i=0;i<24;i++)await Promise.resolve();};
test('the next chart waits for the document scroll end, including a late final pixel',async()=>{
 const s=scenario(),pending=s.run();await flush();
 assert.equal(s.resets(),0,'First positive page movement must not reset a still-running native scroll');
 s.finish({});await flush();assert.equal(s.resets(),0,'A readout scroll end is not the document scroll end');
 s.finish();await pending;assert.equal(s.resets(),1);assert.equal(s.disposed(),1);assert.equal(s.listeners.size,0);
 assert.deepEqual(s.failures,[]);assert.deepEqual(s.cases,['scatter @390: without a readout Page Down scrolls the page']);
});
test('a missing document scroll end is recorded as a failed case and cleans up',async()=>{
 const s=scenario(),pending=s.run();await flush();assert.equal(s.resets(),0);
 const wait = s.waits.at(-1);assert.ok(wait);wait.reject(Error('deliberate scroll-end timeout'));await pending;
 assert.deepEqual(s.failures,['scatter @390: without a readout Page Down scrolls the page: deliberate scroll-end timeout']);
 assert.deepEqual(s.cases,[]);
 // The real check records the failure and continues; its reset must still release the listener.
 assert.equal(s.resets(),1);assert.equal(s.disposed(),1);assert.equal(s.listeners.size,0);
});
