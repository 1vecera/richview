// Host reason (ARC-03): exercise the shipping one-frame probe without opening a browser.
import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import { readTestSource } from './test-source.mts';

const source = readTestSource(new URL('../../../scripts/verify-chart-feedback.ts', import.meta.url));
// Exercise each returned pixel baseline, including the pie reads from the c2 failure.
const helpers = source.slice(source.indexOf('const frames'), source.indexOf('const freeze'));
for (const [name, binding, chart] of [['emphasis', 'full', 'BAR'], ['pie before hover', 'before', 'PIE'], ['pie after hover', 'after', 'PIE']]) {
  const baseline = source.match(new RegExp(`^\\s*const (?:\\[${binding}\\]|${binding})\\s*= .*\\b${chart}\\b.*;$`, 'm'))?.[0];
  assert.ok(baseline, `${name} baseline must remain exercised`);
  for (const delay of [0, 2]) {
    test(`${name} baseline survives ${delay}ms of pre-frame setup`, { timeout: 5000 }, async () => {
      let clockReads = 0;
      const callbacks: (() => void)[] = [], canvas = { frame: 0 };
      const figure = { querySelector: () => canvas };
      const context = vm.createContext({
        BAR: 0, PIE: 0, probes: 0,
        PIXEL: 'globalThis.probes += 1; return [canvas.frame, 17, 29];',
        page: { evaluate: <T, R>(fn: (arg: T) => R, arg: T) => fn(arg) },
        document: { querySelectorAll: () => [figure] },
        performance: { now: () => clockReads++ === 0 ? 0 : canvas.frame ? 20 : delay },
        requestAnimationFrame: (callback: (time: number) => void) => callbacks.push(() => { canvas.frame += 1; callback(20); }),
      });
      vm.runInContext(`${helpers}\nglobalThis.runBaseline = async () => {${baseline}\nreturn ${binding};};`, context);
      // The VM installs the probe slice on its global object.
      const runBaseline: () => Promise<Iterable<number>> = context.runBaseline;
      const pending = runBaseline();
      assert.equal(callbacks.length, 1, 'The baseline must wait for a real frame even after setup exceeds one millisecond');
      const frame = callbacks.shift();
      assert.ok(frame);
      frame();
      assert.deepEqual(Array.from(await pending), [1, 17, 29], 'The probe must read the new frame, not an earlier pixel');
      assert.equal(context.probes, 1);
      assert.equal(callbacks.length, 0);
    });
  }
}
