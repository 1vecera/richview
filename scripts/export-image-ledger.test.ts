// Host reason (ARC-03): verify export evidence against the actual licensed image ledger.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { imageFonts } from './export-image-ledger.ts';

const name = 'openai-vs-anthropic';
const source = join(process.cwd(), 'examples', name);
const ledger = readFileSync(join(source, 'ledger/image-sources.csv'), 'utf8');
const coverDigest = '200206acec579076b51b8fd348f51092202b8115413679bc9663efa7c6bc60a7';

function fixture(fn: (dir: string) => void) {
  mkdirSync('tmp', { recursive: true });
  const dir = mkdtempSync(join(process.cwd(), 'tmp/export-ledger-'));
  try {
    cpSync(join(source, 'assets'), join(dir, 'assets'), { recursive: true });
    mkdirSync(join(dir, 'ledger'));
    writeFileSync(join(dir, 'ledger/image-sources.csv'), ledger);
    fn(dir);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('the actual quoted ledger pins the cover and its embedded fonts', () => {
  const entries = imageFonts(name, source);
  const cover = entries.find(entry => entry.file === 'assets/cover-two-publics.svg');
  assert.ok(cover);
  assert.equal(cover.sha256, coverDigest);
  assert.ok(cover.families.length > 0);
});

test('changed image bytes never inherit ledger font allowances', () => fixture(dir => {
  const path = join(dir, 'assets/cover-two-publics.svg');
  writeFileSync(path, readFileSync(path, 'utf8') + 'changed');
  assert.throws(() => imageFonts(name, dir), /Ledgered image changed/);
}));

test('a hash-matching SVG without font declarations cannot grant an allowance', () => fixture(dir => {
  const path = join(dir, 'assets/cover-two-publics.svg');
  writeFileSync(path, '<svg/>');
  // This digest is for the fixture bytes; the test still requires the font check after hashing.
  const csv = ledger.replace(coverDigest,
    createHash('sha256').update('<svg/>').digest('hex'));
  writeFileSync(join(dir, 'ledger/image-sources.csv'), csv);
  assert.throws(() => imageFonts(name, dir), /no embedded font declarations/);
}));

test('malformed ledger, missing or duplicate columns and invalid hashes fail closed', () => fixture(dir => {
  const path = join(dir, 'ledger/image-sources.csv');
  for (const changed of [
    ledger.replace('id,asset,', 'id,missing,'),
    ledger.replace('id,asset,', 'id,asset,asset,'),
    ledger.replace(coverDigest, 'not-a-sha'),
    ledger.replace('"Original infographic', '""Original infographic'),
    ledger.replace('the data are unchanged."', 'the data are unchanged.'),
  ]) {
    assert.notEqual(changed, ledger, 'Each invalid fixture must change the ledger');
    writeFileSync(path, changed);
    assert.throws(() => imageFonts(name, dir));
  }
}));

test('quoted commas, escaped quotes and CRLF retain the exact digest', () => fixture(dir => {
  const path = join(dir, 'ledger/image-sources.csv');
  writeFileSync(path, ledger.replace('dark-ready art, light type', 'dark-ready ""art"", light type').replace(/\n/g, '\r\n'));
  assert.equal(imageFonts(name, dir).find(entry => entry.file.endsWith('cover-two-publics.svg'))?.sha256, coverDigest);
}));
