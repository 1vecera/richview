// Host reason (ARC-03): check the static browser harness against staged site headers.
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { byteRange, headersFrom, parseHeaderRules, serveStatic } from '../../../scripts/browser-harness.mjs';

function scratch() {
  mkdirSync('tmp', { recursive: true });
  return mkdtempSync(resolve('tmp/header-harness-'));
}

test('site policy drops the old initializer and scopes cross-origin reads', () => {
  const rules = parseHeaderRules(readFileSync('assets/_headers', 'utf8'));
  const cors = rules.filter(rule => rule.headers.some(([name, value]) => name.toLowerCase() === 'access-control-allow-origin' && value === '*')).map(rule => rule.pattern).sort();
  assert.deepEqual(cors, ['/richview_runtime.js', '/richview_runtime_bg.wasm']);
  for (const path of ['/', '/docs/charts', '/playground']) {
    assert.doesNotMatch(headersFrom(rules, path)['content-security-policy'], /'sha256-[^']+'/);
  }
});

test('the server sends the selected staged site policy', async () => {
  const dir = scratch();
  let server;
  try {
    writeFileSync(join(dir, '_headers'), "/\n  Content-Security-Policy: script-src 'sha256-staged'\n");
    writeFileSync(join(dir, 'richview_site_bg.wasm'), 'fixture');
    writeFileSync(join(dir, 'index.html'), '<!doctype html><title>Site</title>');
    server = await serveStatic({ '/': dir });
    const response = await fetch(`${server.base}/`);
    assert.equal(response.headers.get('content-security-policy'), "script-src 'sha256-staged'");
  } finally {
    if (server) await server.close();
    rmSync(dir, { recursive: true });
  }
});

test('an incomplete staged site cannot silently use the source policy', async () => {
  const dir = scratch();
  let server;
  try {
    writeFileSync(join(dir, 'richview_site_bg.wasm'), 'fixture');
    await assert.rejects(serveStatic({ '/': dir }).then(value => { server = value; return value; }), /staged site has no _headers/);
  } finally {
    if (server) await server.close();
    rmSync(dir, { recursive: true });
  }
});

test('a report-only fixture can still use the source policy', async () => {
  const dir = scratch();
  let server;
  try {
    writeFileSync(join(dir, 'report.html'), '<!doctype html><title>Report</title>');
    server = await serveStatic({ '/': dir });
    assert.equal((await fetch(`${server.base}/report.html`)).status, 200);
  } finally {
    if (server) await server.close();
    rmSync(dir, { recursive: true });
  }
});

// WebKit plays a video only from a server that answers byte ranges (S27-959). Every browser suite runs under this
// server, so a request without a range must get exactly the response it got before ranges were answered.
test('a byte range is answered as the host answers it, and a request without one is unchanged', async () => {
  const dir = scratch();
  let server;
  try {
    writeFileSync(join(dir, 'clip.mp4'), '0123456789');
    writeFileSync(join(dir, 'still.avif'), 'avif');
    server = await serveStatic({ '/': dir });
    const policy = { 'content-type': 'video/mp4', 'cache-control': 'no-store', ...headersFrom(parseHeaderRules(readFileSync('assets/_headers', 'utf8')), '/clip.mp4') };
    const own = response => Object.fromEntries([...response.headers].filter(([name]) => !['date', 'connection', 'keep-alive', 'transfer-encoding'].includes(name)));
    const whole = await fetch(`${server.base}/clip.mp4`);
    assert.equal(whole.status, 200);
    assert.deepEqual(own(whole), policy);
    assert.equal(await whole.text(), '0123456789');
    const part = await fetch(`${server.base}/clip.mp4`, { headers: { range: 'bytes=2-5' } });
    assert.equal(part.status, 206);
    assert.deepEqual(own(part), { ...policy, 'accept-ranges': 'bytes', 'content-range': 'bytes 2-5/10', 'content-length': '4' });
    assert.equal(await part.text(), '2345');
    const past = await fetch(`${server.base}/clip.mp4`, { headers: { range: 'bytes=10-' } });
    assert.equal(past.status, 416);
    assert.equal(past.headers.get('content-range'), 'bytes */10');
    assert.equal(await past.text(), '');
    assert.equal((await fetch(`${server.base}/still.avif`)).headers.get('content-type'), 'image/avif');
  } finally {
    if (server) await server.close();
    rmSync(dir, { recursive: true });
  }
});

test('byteRange reads one byte range and leaves what it cannot honour to a whole response', () => {
  assert.deepEqual(byteRange('bytes=0-1', 10), { start: 0, end: 1 });
  assert.deepEqual(byteRange('bytes=4-', 10), { start: 4, end: 9 });
  assert.deepEqual(byteRange('bytes=5-99', 10), { start: 5, end: 9 });
  assert.deepEqual(byteRange('bytes=-3', 10), { start: 7, end: 9 });
  assert.deepEqual(byteRange('bytes=-30', 10), { start: 0, end: 9 });
  for (const header of ['bytes=10-', 'bytes=12-15', 'bytes=-0']) assert.equal(byteRange(header, 10), false, header);
  for (const header of [undefined, '', 'bytes=-', 'bytes=5-2', 'bytes=0-1,4-5', 'items=0-1', 'bytes= 0-1']) assert.equal(byteRange(header, 10), undefined, header);
});
