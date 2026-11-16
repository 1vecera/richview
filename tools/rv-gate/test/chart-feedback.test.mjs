// Host reason (ARC-03): exercise the shipping one-frame probe without opening a browser.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('../../../scripts/verify-chart-feedback.mjs', import.meta.url), 'utf8');
// Exercise each returned pixel baseline, including the pie reads from the c2 failure.
const helpers = source.slice(source.indexOf('const frames ='), source.indexOf('const freeze ='));
for (const [name, binding, chart] of [['emphasis', 'full', 'BAR'], ['pie before hover', 'before', 'PIE'], ['pie after hover', 'after', 'PIE']]) {
  const baseline = source.match(new RegExp(`^\\s*const (?:\\[${binding}\\]|${binding}) = .*\\b${chart}\\b.*;$`, 'm'))?.[0];
  assert.ok(baseline, `${name} baseline must remain exercised`);
  for (const delay of [0, 2]) {
    test(`${name} baseline survives ${delay}ms of pre-frame setup`, { timeout: 5000 }, async () => {
      let clockReads = 0;
      const callbacks = [], canvas = { frame: 0 };
      const figure = { querySelector: () => canvas };
      const context = vm.createContext({
        BAR: 0, PIE: 0, probes: 0,
        PIXEL: 'globalThis.probes += 1; return [canvas.frame, 17, 29];',
        page: { evaluate: (fn, arg) => fn(arg) },
        document: { querySelectorAll: () => [figure] },
        performance: { now: () => clockReads++ === 0 ? 0 : canvas.frame ? 20 : delay },
        requestAnimationFrame: callback => callbacks.push(() => { canvas.frame += 1; callback(20); }),
      });
      vm.runInContext(`${helpers}\nglobalThis.runBaseline = async () => {${baseline}\nreturn ${binding};};`, context);
      const pending = context.runBaseline();
      assert.equal(callbacks.length, 1, 'The baseline must wait for a real frame even after setup exceeds one millisecond');
      callbacks.shift()();
      assert.deepEqual(Array.from(await pending), [1, 17, 29], 'The probe must read the new frame, not an earlier pixel');
      assert.equal(context.probes, 1);
      assert.equal(callbacks.length, 0);
    });
  }
}
