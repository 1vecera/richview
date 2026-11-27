// Host reason (ARC-03): check the hostile-browser corpus boundary without a staged runtime.
// Live malicious/benign controls are mandatory in the registered release browser command.
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { loadHostileCorpus } from '../../../scripts/verify-hostile-browser.ts';

function fixture() {
  const parent = resolve('tmp');
  mkdirSync(parent, { recursive: true });
  const directory = mkdtempSync(join(parent, 'hostile-index-'));
  const entries = [{ case: 'one', sink: 'source text', expect: [] }, { case: 'two', sink: 'style input', expect: [], browser_refusal_codes: ['report.asset.missing'] }];
  const index = () => writeFileSync(join(directory, 'index.json'), JSON.stringify({ cases: entries }));
  writeFileSync(join(directory, 'one.rmd'), 'Escaped code: `HOSTILE_CODE()`');
  writeFileSync(join(directory, 'one.csv'), 'label\nHOSTILE_CSV()');
  writeFileSync(join(directory, 'two.rmd'), 'Body');
  writeFileSync(join(directory, 'two.rstyle.yaml'), 'tokens:\n  bg: "HOSTILE_STYLE()"');
  index();
  return { directory, entries, index, close: () => rmSync(directory, { recursive: true, force: true }) };
}

test('hostile browser reads actual indexed sources, their own companions and explicit native refusals', () => {
  const f = fixture();
  try {
    const cases = loadHostileCorpus(f.directory);
    assert.deepEqual(cases.map((item: { contract: { case: string } }) => item.contract.case), ['one', 'two']);
    assert.deepEqual(cases[0].companions, ['one.csv']);
    assert.deepEqual(cases[0].canaries, ['HOSTILE_CODE', 'HOSTILE_CSV']);
    assert.deepEqual(cases[1].companions, ['two.rstyle.yaml']);
    assert.deepEqual(cases[1].canaries, ['HOSTILE_STYLE']);
    assert.deepEqual(cases[1].contract.browser_refusal_codes, ['report.asset.missing']);
  } finally { f.close(); }
});

test('hostile browser refuses corpus drift and ambiguous native-refusal declarations before rendering', () => {
  for (const defect of ['omitted', 'missing', 'duplicate', 'escape', 'refusal']) {
    const f = fixture();
    try {
      if (defect === 'omitted') writeFileSync(join(f.directory, 'unlisted.rmd'), 'not indexed');
      if (defect === 'missing') rmSync(join(f.directory, 'one.rmd'));
      if (defect === 'duplicate') { f.entries[1].case = 'one'; f.index(); }
      if (defect === 'escape') { f.entries[1].case = '../outside'; f.index(); }
      if (defect === 'refusal') { f.entries[1].browser_refusal_codes = ['anything-goes']; f.index(); }
      assert.throws(() => loadHostileCorpus(f.directory), defect === 'duplicate' ? /unique/ : defect === 'escape' ? /file stems/ : defect === 'refusal' ? /refusal codes/ : /every report/);
    } finally { f.close(); }
  }
});
