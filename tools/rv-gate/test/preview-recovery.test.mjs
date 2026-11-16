// Host reason (ARC-03): exercise the browser suite's bounded preview recovery.
import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import { capturePreviewFailure, settleRestoredPreview } from '../../../scripts/preview-recovery.mjs';

function fakeRestore(settle, afterReady = () => {}) {
  let generation = 1, waits = 0, frames = 0, calls = 0, predicate;
  const first = { name: 'initial' }, replacement = { name: 'replacement' };
  const page = {
    async waitForFunction(fn, arg, options) {
      predicate = fn; waits++; assert.equal(arg, null); assert.equal(options.timeout, 60_000);
      const readyGeneration = generation;
      afterReady({ waits, replace() { generation++; } });
      return { async jsonValue() { return readyGeneration; }, async dispose() {} };
    },
    async evaluate() { return generation; },
  };
  const previewFrame = async () => { frames++; return replacement; };
  const settledCharts = async (preview, count) => {
    calls++; assert.equal(count, 1);
    return settle({ preview, calls, replace() { generation++; } });
  };
  return {
    run: () => settleRestoredPreview(page, first, previewFrame, settledCharts),
    stats: () => ({ waits, frames, calls, predicate }), first, replacement,
  };
}

function predicateResult(predicate, state, now, srcdoc = '0.985 0.004 70') {
  return vm.runInNewContext(`(${predicate.toString()})()`, {
    window: { __rvEvidence: { sourceRendering: state } },
    document: { querySelector: () => ({ getAttribute: () => srcdoc }) },
    performance: { now: () => now },
  });
}

test('restore waits for the restored style and 500 ms of loaded, unchanged srcdoc', async () => {
  const fake = fakeRestore(() => {});
  assert.equal(await fake.run(), fake.first);
  const { predicate } = fake.stats();
  const state = { generation: 2, loadedGeneration: 2, rendered: 1, loaded: 1, changedAt: 100, loadedAt: 200 };
  assert.equal(predicateResult(predicate, state, 699), false);
  assert.equal(predicateResult(predicate, state, 700), 2);
  assert.equal(predicateResult(predicate, { ...state, loaded: -1 }, 1000), false);
  assert.equal(predicateResult(predicate, { ...state, loadedGeneration: 1 }, 1000), false);
  assert.equal(predicateResult(predicate, { ...state, loadedAt: null }, 1000), false);
  assert.equal(predicateResult(predicate, state, 1000, 'old style'), false);
});

test('a detached document retries once only after a later srcdoc mutation', async () => {
  for (const message of [
    'Element is not attached to the DOM', 'Execution context was destroyed',
    'Frame was detached', 'Cannot find context with specified id',
  ]) {
    const fake = fakeRestore(({ calls, replace }) => {
      if (calls === 1) { replace(); throw Error(message); }
    });
    assert.equal(await fake.run(), fake.replacement);
    const { waits, frames, calls } = fake.stats();
    assert.deepEqual({ waits, frames, calls }, { waits: 2, frames: 1, calls: 2 });
  }
});

test('a replacement just after the quiet check is not mistaken for the settled document', async () => {
  const fake = fakeRestore(
    ({ calls }) => { if (calls === 1) throw Error('Element is not attached to the DOM'); },
    ({ waits, replace }) => { if (waits === 1) replace(); },
  );
  assert.equal(await fake.run(), fake.replacement);
  assert.equal(fake.stats().calls, 2);
});

test('a real chart error and a detachment without mutation both stay red', async () => {
  const real = fakeRestore(({ replace }) => { replace(); throw Error('chart is genuinely missing'); });
  await assert.rejects(real.run(), /chart is genuinely missing/);
  assert.equal(real.stats().calls, 1);
  const unchanged = fakeRestore(() => { throw Error('Element is not attached to the DOM'); });
  await assert.rejects(unchanged.run(), /Element is not attached/);
  assert.equal(unchanged.stats().calls, 1);
});

test('a second detachment is not retried', async () => {
  const fake = fakeRestore(({ replace }) => { replace(); throw Error('Execution context was destroyed'); });
  await assert.rejects(fake.run(), /Execution context was destroyed/);
  assert.equal(fake.stats().calls, 2);
  assert.equal(fake.stats().waits, 2);
});

test('a quiet-window timeout names the failed restore without hiding its cause', async () => {
  const timeout = Object.assign(Error('waiting failed'), { name: 'TimeoutError' });
  const page = { async waitForFunction() { throw timeout; } };
  await assert.rejects(settleRestoredPreview(page, {}, () => {}, () => {}), error =>
    /stay loaded and unchanged for 500 ms/.test(error.message) && error.cause === timeout);
});

test('a timed-out failure snapshot does not change when frame probes later reject', async () => {
  const rejections = [];
  const frame = {
    evaluate: () => new Promise((_, reject) => rejections.push(reject)),
    isDetached: () => false, url: () => 'about:srcdoc', parentFrame: () => null,
  };
  const page = {
    evaluate: async () => ({ hasExpected: true }), frames: () => [frame],
    locator: () => ({ elementHandle: async () => ({ contentFrame: async () => frame, dispose: async () => {} }) }),
  };
  const snapshot = await capturePreviewFailure(page, frame, 'Race C', Error('timeout'));
  assert.equal(snapshot.snapshotTimeout, true);
  assert.equal(rejections.length, 2);
  const atDecision = structuredClone(snapshot);
  for (const reject of rejections) reject(Error('Frame was detached'));
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(snapshot, atDecision);
  assert.equal(Object.hasOwn(snapshot, 'currentFrame'), false);
  assert.equal(Object.hasOwn(snapshot, 'retainedFrame'), false);
});
