// Host reason (ARC-03): render the committed hostile corpus and verify actual Chromium safety effects.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { auditHostileReport } from './hostile-browser.mjs';
import { cli, harness, root, serveStatic, waitForBenchmark } from './browser-harness.mjs';

/** Read the authoritative index and reject an omitted, duplicated or missing report before rendering. */
export function loadHostileCorpus(directory) {
  const index = JSON.parse(readFileSync(join(directory, 'index.json'), 'utf8'));
  const names = index.cases.map(item => item.case);
  assert.ok(names.length && new Set(names).size === names.length, 'Hostile corpus case names must be unique and nonempty');
  assert.ok(names.every(name => /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(name)), 'Hostile corpus case names must be file stems');
  const files = readdirSync(directory);
  assert.deepEqual([...names].sort(), files.filter(name => name.endsWith('.rmd')).map(name => name.slice(0, -4)).sort(), 'Hostile index must name every report exactly once');
  return index.cases.map(contract => {
    assert.ok(typeof contract.sink === 'string' && contract.sink.trim(), `${contract.case}: missing sink`);
    const source = readFileSync(join(directory, `${contract.case}.rmd`), 'utf8');
    const companions = files.filter(name => name.startsWith(`${contract.case}.`) && !name.endsWith('.rmd'));
    const text = [source, ...companions.map(name => readFileSync(join(directory, name), 'utf8'))].join('\n');
    const canaries = [...new Set([...text.matchAll(/\b(HOSTILE[A-Z0-9_]*)\s*\(/g)].map(match => match[1]))];
    if (contract.browser_refusal_codes !== undefined) {
      assert.ok(Array.isArray(contract.browser_refusal_codes) && contract.browser_refusal_codes.length
        && contract.browser_refusal_codes.every(code => typeof code === 'string' && /^report\.asset\.(?:missing|escape)$/.test(code)), `${contract.case}: invalid browser refusal codes`);
    }
    if (contract.browser_charts !== undefined) assert.ok(Number.isSafeInteger(contract.browser_charts) && contract.browser_charts > 0, `${contract.case}: invalid browser chart expectation`);
    return { contract, source, companions, canaries };
  });
}

function permissive(html) {
  let removed = 0;
  const result = html.replace(/<meta\b[^>]*>/gi, tag => {
    if (!/http-equiv\s*=\s*["']Content-Security-Policy["']/i.test(tag)) return tag;
    removed++;
    return '';
  });
  assert.equal(removed, 1, 'Control must remove the actual report CSP before injecting an executable payload');
  return result;
}

/** Run the real native corpus and mandatory real-browser controls; no partial selector is supported. */
export async function runHostileBrowser({ bin = cli, out }) {
  const corpusDir = join(root, 'crates/richview-core/tests/hostile');
  const corpus = loadHostileCorpus(corpusDir);
  await waitForBenchmark();
  const started = performance.now(), deadline = started + 60_000;
  const reports = [], controls = [], failures = [];
  let h, server, timer, timedOut = false;
  const result = () => ({
    engine: 'chromium', seconds: (performance.now() - started) / 1000,
    cohort: corpus.map(item => item.contract.case), reports, controls, failures,
    complete: reports.length === corpus.length && reports.every(item => item.outcome === 'refused' || item.browser?.checkpoints.includes('final')) && controls.length === 6,
  });
  mkdirSync(out, { recursive: true });
  try {
    h = await harness('hostile-browser');
    assert.equal(h.engine, 'chromium', 'Hostile browser proof requires Chromium');
    timer = setTimeout(() => {
      timedOut = true;
      void h.browser.close().catch(error => failures.push({ kind: 'cleanup', message: error.message }));
    }, Math.max(1, deadline - performance.now()));
    const pages = new Map();
    for (const item of corpus) {
      const { contract, source, companions } = item;
      assert.ok(performance.now() < deadline, 'Hostile browser one-minute deadline exceeded');
      const folder = join(h.dir, 'reports', contract.case);
      mkdirSync(folder, { recursive: true });
      writeFileSync(join(folder, 'report.rmd'), source);
      for (const file of companions) copyFileSync(join(corpusDir, file), join(folder, file));
      const native = spawnSync(bin, ['generate', join(folder, 'report.rmd'), '-o', 'report.html', '--format', 'json'], {
        cwd: root, encoding: 'utf8', env: process.env, maxBuffer: 64 << 20,
        timeout: Math.max(1, Math.floor(deadline - performance.now())),
      });
      assert.ok([0, 1].includes(native.status), `${contract.case}: native generator failed: ${native.error?.message ?? native.stderr}`);
      const generated = JSON.parse(native.stdout);
      const path = join(folder, 'Rendered/report.html');
      const record = { case: contract.case, sink: contract.sink, nativeExit: native.status, diagnostics: generated.diagnostics ?? [] };
      reports.push(record);
      if (contract.browser_refusal_codes) {
        const errors = record.diagnostics.filter(diagnostic => diagnostic.level === 'error').map(diagnostic => diagnostic.code);
        assert.equal(native.status, 1, `${contract.case}: expected an explicit native refusal`);
        assert.equal(existsSync(path), false, `${contract.case}: refused source produced HTML`);
        assert.deepEqual([...new Set(errors)].sort(), [...contract.browser_refusal_codes].sort(), `${contract.case}: refusal reason changed`);
        record.outcome = 'refused';
      } else {
        assert.ok(existsSync(path), `${contract.case}: unexpected missing HTML`);
        for (const code of contract.expect ?? []) assert.ok(record.diagnostics.some(diagnostic => diagnostic.code === code), `${contract.case}: missing diagnostic ${code}`);
        const html = readFileSync(path, 'utf8');
        // A written draft is held to the same text rules as the core render (FEA-RP-30), and every
        // local target it links or loads stays inside the report folder.
        for (const text of contract.absent ?? []) assert.ok(!html.includes(text), `${contract.case}: the written draft contains ${JSON.stringify(text)}`);
        for (const [, target] of html.matchAll(/\s(?:href|src|action|formaction|poster)="([^"]*)"/gi)) {
          if (/^(?:https?:|mailto:|tel:|data:|#)/i.test(target)) continue;
          const raw = target.replace(/&amp;/g, '&');
          let path = raw;
          try { path = decodeURIComponent(raw); } catch { /* a malformed escape is judged as written */ }
          const outside = /^[/\\]/.test(path) || path.includes('\\') || /(?:^|\/)\.\.(?:\/|$)/.test(path);
          assert.ok(!outside, `${contract.case}: the written draft points outside the report folder: ${target}`);
        }
        record.outcome = 'rendered';
        record.htmlSha256 = createHash('sha256').update(html).digest('hex');
        pages.set(`/reports/${contract.case}`, html);
      }
    }
    const base = pages.get('/reports/s01-opn16-palette');
    assert.ok(base?.includes('data-rv-payload'), 'The controls need an actual native report with a real chart runtime');
    const add = (html, body) => html.replace('</body>', `${body}</body>`);
    const controlPages = [
      ['script', add(permissive(base), '<script>HOSTILE("executed-control")</script>')],
      ['style-markup', add(permissive(base), '<style>#rv-hostile-control { color:rgb(1,2,3) }</style><div id="rv-hostile-control">intrusion</div><script>HOSTILE(getComputedStyle(document.getElementById("rv-hostile-control")).color)</script>')],
      ['http', add(permissive(base), '<img src="/control-egress">')],
      ['websocket', add(permissive(base), '<script>new WebSocket(`ws://${location.host}/control-egress`)</script>')],
      ['csp', add(base, '<script>HOSTILE("must-be-blocked")</script>')],
      ['runtime', base.replace(/<script\b[^>]*>[\s\S]*?<\/script>/g, script => script.includes('richview_runtime') ? '' : script)],
    ];
    assert.notEqual(controlPages[5][1], base, 'Missing-runtime control must remove the actual native runtime');
    for (const [name, html] of controlPages) pages.set(`/controls/${name}`, html);
    let egress = 0;
    server = await serveStatic({}, { route(path, _, response) {
      if (path === '/control-egress') { egress++; response.writeHead(200); response.end('escaped'); return true; }
      const html = pages.get(path);
      if (html === undefined) return false;
      response.writeHead(200, { 'content-type': 'text/html', 'cache-control': 'no-store' });
      response.end(html);
      return true;
    } });
    const canaries = [...new Set(['HOSTILE', ...corpus.flatMap(item => item.canaries)])];
    for (const item of corpus) {
      const record = reports.find(record => record.case === item.contract.case);
      if (record.outcome === 'refused') continue;
      record.browser = await auditHostileReport(h.browser, { url: `${server.base}/reports/${record.case}`, contract: item.contract, canaries, deadline });
      for (const finding of record.browser.findings) failures.push({ case: record.case, ...finding });
      if (timedOut) throw new Error('Hostile browser one-minute deadline exceeded');
    }
    const counts = reports.reduce((total, record) => ({ charts: total.charts + (record.browser?.charts ?? 0), tabs: total.tabs + (record.browser?.tabs ?? 0), readouts: total.readouts + (record.browser?.readouts ?? 0), tables: total.tables + (record.browser?.tables ?? 0) }), { charts: 0, tabs: 0, readouts: 0, tables: 0 });
    assert.ok(counts.charts > 0 && counts.tabs > 0 && counts.readouts > 0 && counts.tables > 0, 'The corpus must exercise real chart, tab, readout and Table hydration');
    for (const [name] of controlPages) {
      const observed = await auditHostileReport(h.browser, { url: `${server.base}/controls/${name}`, contract: {}, canaries, deadline });
      const kinds = new Set(observed.findings.map(finding => finding.kind));
      let caught;
      if (name === 'script') caught = observed.findings.some(finding => finding.kind === 'script' && finding.name === 'HOSTILE' && finding.args[0] === 'executed-control');
      if (name === 'style-markup') caught = kinds.has('style') && kinds.has('markup') && observed.findings.some(finding => finding.kind === 'script' && finding.args?.[0] === 'rgb(1, 2, 3)');
      if (name === 'http') caught = observed.findings.some(finding => finding.kind === 'network' && finding.transport === 'image');
      if (name === 'websocket') caught = observed.findings.some(finding => finding.kind === 'network' && finding.transport === 'websocket');
      if (name === 'csp') caught = kinds.has('csp') && !kinds.has('script');
      if (name === 'runtime') caught = kinds.has('runtime') && !observed.checkpoints.includes('hydrated');
      else caught = caught && observed.checkpoints.includes('final');
      controls.push({ name, caught: !!caught, observed });
      if (!caught) failures.push({ control: name, kind: 'control', message: 'The browser guard did not detect its actual injected defect' });
    }
    assert.equal(egress, 0, 'A control request reached the loopback egress sentinel');
  } catch (error) { failures.push({ kind: 'runner', message: error.message }); }
  finally {
    clearTimeout(timer);
    for (const close of [() => h?.close(), () => server?.close()]) {
      try { await close(); } catch (error) { failures.push({ kind: 'cleanup', message: error.message }); }
    }
    const summary = result();
    if (!summary.complete && !failures.some(failure => failure.kind === 'runner')) failures.push({ kind: 'runner', message: 'Hostile corpus or its browser controls are incomplete' });
    writeFileSync(join(out, 'hostile-browser.json'), JSON.stringify(result(), null, 2) + '\n');
    if (h) {
      copyFileSync(join(h.dir, 'browser-environment.json'), join(out, 'browser-environment.json'));
      rmSync(h.dir, { recursive: true, force: true });
    }
  }
  return result();
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const option = name => { const index = process.argv.indexOf(name); return index < 0 ? undefined : process.argv[index + 1]; };
  const out = option('--out');
  if (!out) throw new Error('usage: verify-hostile-browser.mjs --out <directory> [--bin <richview>]');
  const summary = await runHostileBrowser({ bin: option('--bin') ?? cli, out: resolve(out) });
  console.log(`${summary.failures.length ? 'FAIL' : 'PASS'} hostile browser: ${summary.reports.length}/${summary.cohort.length} reports, ${summary.controls.filter(control => control.caught).length}/${summary.controls.length} controls, ${summary.seconds.toFixed(2)}s`);
  for (const failure of summary.failures) console.error(JSON.stringify(failure));
  process.exitCode = summary.failures.length || !summary.complete ? 1 : 0;
}
