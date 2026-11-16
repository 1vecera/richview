// Host reason (ARC-03): exercise the loop's decisions without running or promoting a release.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('../full-loop-controller.mjs', import.meta.url), 'utf8');
const extract = (start, end) => {
 const a = source.indexOf(start), b = source.indexOf(end, a);
 assert.ok(a >= 0 && b > a);
 return source.slice(a, b);
};

test('an externally promoted head is held without a run', () => {
 const head = 'a'.repeat(40);
 const c = vm.createContext({fs: {existsSync: () => false, readdirSync: () => []}, root: '/tmp', gateHome: () => '/gate', receiptNotes: () => [], Date});
 vm.runInContext(extract('function registrationRefusal(', 'function promote(') + 'globalThis.hold=headHold;', c);
 assert.match(c.hold(head, 'affected', head).reason, /pre-prod/);
});

test('a refused passing receipt is recorded once and holds until dev changes, including after restart', () => {
 const head = 'a'.repeat(40), files = new Map(), messages = [];
 const fs = {existsSync: path => path.includes('/bin-release/') || files.has(path), readFileSync: path => files.get(path), readdirSync: () => [], appendFileSync() {}};
 const c = vm.createContext({fs, root: '/tmp', gateHome: () => '/gate', receiptNotes: () => [], launcher: '/launcher', LEAD: 'lead', Date,
  save: (path, value) => files.set(path, JSON.stringify(value)), notify: (...args) => messages.push(args), console: {log() {}},
  promoteWithCurrentRunner: () => { throw Error('Affected receipt green anchor differs'); }});
 vm.runInContext('const RERUN_AFTER_MS=86400000;' + extract('function registrationRefusal(', 'function changedFailures(') + 'globalThis.hold=headHold;globalThis.promote=promote;globalThis.refusal=registrationRefusal;', c);
 const active = {head, receipt: 'affected-20261112T070156Z-aaaaaaaaaa'};
 c.promote(active, {result: 'pass'});
 for (let i = 0; i < 3; i++) assert.match(c.hold(head, 'affected', 'b'.repeat(40)).reason, /Affected receipt green anchor differs/);
 assert.equal(messages.length, 1);
 assert.equal(c.hold('c'.repeat(40), 'affected', 'b'.repeat(40)), null);
 const calls=[];
 const restart=vm.createContext({prior: active, file: '/receipt', fs: {existsSync: () => true, readFileSync: () => '{"result":"pass"}'},
  receiptNotes: () => [], registrationRefusal: c.refusal, considerJudgment: () => calls.push('judgment'), promote: () => calls.push('registration'), console: {log() {}}});
 vm.runInContext(extract("  if(fs.existsSync(file)&&JSON.parse(fs.readFileSync(file,'utf8')).result==='pass'){", '\n }\n const START_GIB=15;'), restart);
 assert.deepEqual(calls, []);
});
