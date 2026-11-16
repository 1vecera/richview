// Host reason (ARC-03): exercise export producer admission in real script processes before any browser starts.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import test from 'node:test';

const diagnostic = result => JSON.stringify({
  status: result.status, signal: result.signal, pid: result.pid, elapsedMs: result.elapsedMs,
  error: result.error && { name: result.error.name, message: result.error.message, code: result.error.code, errno: result.error.errno, syscall: result.error.syscall, spawnargs: result.error.spawnargs },
  prerequisites: result.prerequisites, invocation: result.invocationContents, stdout: result.stdout, stderr: result.stderr,
});

test('the mixed-media code guard runs and rejects a missing fixture', async () => {
  const source = readFileSync(resolve(process.env.EXPORT_PRODUCER_UNDER_TEST || 'scripts/verify-exports.mjs'), 'utf8');
  // Run the shipping guard with its own assertion import and fixture, without PDF/browser startup.
  const imports = source.match(/^import .* from 'node:assert\/strict';$/gm) ?? [];
  const fixture = source.slice(source.indexOf('const codeText ='), source.indexOf('\n\nfunction prepareReport'));
  const guard = source.match(/^\s*if \(name === 'mixed-media'\) assert\.ok\(.*;$/m)?.[0];
  assert.ok(guard, 'The mixed-media fixture guard must be exercised');
  const module = await import('data:text/javascript;base64,' + Buffer.from([
    ...imports, fixture, 'export { paperCode };',
    'export function check(fact) { const name = "mixed-media";', guard, '}',
  ].join('\n')).toString('base64'));
  module.check({ code: [module.paperCode] });
  assert.throws(() => module.check({ code: [module.paperCode.slice(0, -1)] }), {
    code: 'ERR_ASSERTION', message: 'The long-code regression fixture must reach the rendered report',
  });
});

test('export producer requires matching admission and keeps partial receipts separate', () => {
  const root = process.cwd(), script = resolve(process.env.EXPORT_PRODUCER_UNDER_TEST || 'scripts/verify-exports.mjs');
  mkdirSync(join(root, 'tmp'), { recursive: true });
  const dir = mkdtempSync(join(root, 'tmp/export-producer-test-'));
  try {
    const bin = join(dir, 'bin'), dist = join(dir, 'prerequisite-shape');
    mkdirSync(bin); mkdirSync(dist);
    // No payload is loaded: the CLI fixture always stops before native/PW startup.
    writeFileSync(join(dist, 'richview_site_bg.wasm'), '');
    // These utilities only report presence/status; avoid seven unrelated Node bootstraps per run.
    const tool = '#!/bin/sh\nprintf \'{"tool":"%s","pid":%s}\\n\' "${0##*/}" "$$" >> "$EXPORT_PREREQUISITES"\ncase "${0##*/}" in unshare) exit 1;; *) exit 0;; esac\n';
    for (const name of ['pdfinfo', 'pdftotext', 'pdftoppm', 'pdffonts', 'qpdf', 'magick', 'unshare']) { const path = join(bin, name); writeFileSync(path, tool); chmodSync(path, 0o755); }
    const cli = join(bin, 'richview');
    writeFileSync(cli, '#!/usr/bin/env node\nimport{writeFileSync}from"node:fs";writeFileSync(process.env.EXPORT_INVOCATION,JSON.stringify(process.argv.slice(2)));console.error(process.env.EXPORT_RELAUNCH_EVENT);console.log(JSON.stringify({ok:false,error:"deliberate stop before browser startup"}));process.exit(1);\n'); chmodSync(cli, 0o755);
    const run = (name, parent) => {
      const out = join(dir, name), invocation = out + '.cli.json', prerequisites = out + '.tools.jsonl';
      const env = { ...process.env, PATH: bin + ':' + process.env.PATH, RICHVIEW_BIN: cli, RICHVIEW_DIST: dist, RICHVIEW_VERIFY_SHARD: '7/8', EXPORT_INVOCATION: invocation, EXPORT_PREREQUISITES: prerequisites, EXPORT_RELAUNCH_EVENT: 'RICHVIEW_BROWSER_RELAUNCH '+JSON.stringify({version:1,event:'browser-startup-relaunch',surface:'pdf',engine:'chromium',attempt:2,maxAttempts:2,timeoutMs:15000}),
        // Isolate only the read-only quiet marker for this no-browser fixture; no Cargo output or target is created.
        RV_SHARED_TARGET: dir };
      delete env.RICHVIEW_VERIFY_PARENT; if (parent) env.RICHVIEW_VERIFY_PARENT = parent;
      const started = Date.now();
      const result = spawnSync(process.execPath, [script, '--reports', 'first-report', '--themes', 'sleek', '--out', out], { cwd: root, env, encoding: 'utf8', timeout: 60000 }); // a loaded host needs more than 15 s to start the script
      return { ...result, out, invocation, elapsedMs: Date.now() - started,
        prerequisites: existsSync(prerequisites) ? readFileSync(prerequisites, 'utf8').trim().split('\n').filter(Boolean) : [],
        invocationContents: existsSync(invocation) ? readFileSync(invocation, 'utf8') : null };
    };
    for (const parent of [undefined, 'runtime']) {
      const whole = run(parent || 'bare', parent);
      assert.equal(whole.status, 1, diagnostic(whole)); assert.match(whole.stderr, /deliberate stop before browser startup/);
      assert.equal(whole.stderr.split('\n').filter(line=>line.startsWith('RICHVIEW_BROWSER_RELAUNCH ')).length,1);
      assert.ok(JSON.parse(readFileSync(whole.invocation)).some(value => value.endsWith('/first-report/report.rmd')));
    }
    const partial = run('admitted', 'exports');
    assert.equal(partial.status, 0, diagnostic(partial)); assert.equal(existsSync(partial.invocation), false);
    const receipt = JSON.parse(readFileSync(join(partial.out, 'exports-7-of-8.json')));
    assert.equal(receipt.admission, 'exports'); assert.equal(receipt.shard.count, 8); assert.deepEqual(receipt.completed, []); assert.deepEqual(receipt.results, []);
    assert.equal(existsSync(join(partial.out, 'exports.json')), false); assert.equal(existsSync(join(partial.out, 'types.json')), false);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the PDF code guard ignores running feet across pages and rejects changed body code', () => {
  const source = readFileSync(resolve(process.env.EXPORT_PRODUCER_UNDER_TEST || 'scripts/verify-exports.mjs'), 'utf8');
  const helperStart = source.indexOf('function codeBody(');
  const owner = source.slice(source.indexOf('function analyze('), source.indexOf("\n// A theme's"));
  const helper = helperStart < 0 ? '' : source.slice(helperStart, source.indexOf('function analyze('));
  const codeText = source.match(/^const codeText = .*;$/m)[0];
  const code = 'const Count = records.items().length;';
  const parts = [{ page: 1, y: 100, text: 'const Count = records.' }, { page: 1, y: 780, text: 'Running title 1 / 2' },
    { page: 2, y: 100, text: 'items().length;' }, { page: 2, y: 780, text: 'Running title 2 / 2' }];
  const extract = (records, top = 0, bottom = 800) => [1, 2].map(page => records.filter(part => part.page === page && part.y >= top && part.y < bottom).map(part => part.text).join('\n')).join('\f');
  const inspect = (records, expected = [code]) => {
    const calls = [];
    const run = (file, args) => {
      calls.push({ file, args });
      if (file === 'pdfinfo') return 'Pages: 2\nPage size: 600 x 800 pts (A4)\nTagged: yes';
      if (file === 'qpdf') return args.includes('--json') ? '{"outlines":[{}]}' : '';
      assert.equal(file, 'pdftotext');
      if (!args.includes('-H')) return extract(records);
      const value = key => Number(args[args.indexOf(key) + 1]);
      assert.equal(value('-r'), 72); assert.equal(value('-x'), 0); assert.equal(value('-W'), 600);
      assert.equal(value('-y'), 76); assert.equal(value('-H'), 645);
      return extract(records, value('-y'), value('-y') + value('-H'));
    };
    // Execute the shipping analyzer, not a copy of its code completeness predicate.
    const analyze = new Function('run', 'norm', 'readFileSync', 'rmSync', 'rasters', 'fontsOf', 'statSync', codeText + '\n' + helper + owner + '\nreturn analyze;')(
      run, text => text.toLowerCase().replace(/\s+/g, ''), () => '', () => {},
      () => [{ luminance: 1, gap: 0.1 }, { luminance: 1, gap: 0.1 }], () => [], () => ({ size: 1000 }));
    const answer = analyze('report.pdf', { headings: [], figures: [], tables: [], links: 0, code: expected, math: 0 }, { foot: 'Running title', margins: [0.1, 0.1], rasterDir: 'unused' });
    assert.equal(answer.text, extract(records), 'Raw PDF text remains available for unchanged determinism checks');
    return { ...answer.result, crops: calls.filter(call => call.args.includes('-H')).length };
  };
  const complete = inspect(parts);
  assert.deepEqual(complete.code_missing, [], 'Complete code may continue after a running page foot');
  assert.deepEqual(complete.failures, []);
  for (const replacement of ['item().length;', 'Items().length;', 'items().length']) {
    const changed = inspect(parts.map(part => part.page === 2 && part.y === 100 ? { ...part, text: replacement } : part));
    assert.deepEqual(changed.code_missing, [code]);
    assert.deepEqual(changed.failures, ['1 code blocks are incomplete in PDF text']);
  }
  const footerOnly = inspect([{ page: 1, y: 780, text: code }]);
  assert.deepEqual(footerOnly.code_missing, [code], 'The running foot cannot supply missing body code');
  const noCode = inspect(parts, []);
  assert.deepEqual(noCode.code_missing, []); assert.equal(noCode.crops, 0);
});
