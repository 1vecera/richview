// Host reason (ARC-03): filesystem/Git/CLI contracts exercise the checker as the gate calls it.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, unlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { checkContracts } from '../check.mjs';

function fixture(t) {
  mkdirSync('tmp', { recursive: true });
  const root = mkdtempSync(join(process.cwd(), 'tmp/api-check-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const dir of ['src', 'snapshots', 'chart']) mkdirSync(join(root, dir));
  const contract = { id: 'core.document', producer: 'richview-core', kind: 'rust', module: 'richview_core::document', source: 'src/document.rs', crateRoot: 'src/lib.rs', snapshot: 'snapshots/core.json', consumers: ['richview-app'] };
  const snapshot = { schema: 'interface-v1', id: 'core.document', producer: 'richview-core', kind: 'rust', surface: { module: 'richview_core::document', registration: 'pub mod document ;', exports: [{ name: 'read', kind: 'fn', declaration: 'pub fn read ( source : & str ) - > usize' }], declarations: [] } };
  const write = (file, value) => writeFileSync(join(root, file), typeof value === 'string' ? value : JSON.stringify(value));
  write('src/lib.rs', 'pub mod document;');
  write('src/document.rs', 'pub fn read(source: &str) -> usize { source.len() }');
  write('snapshots/core.json', snapshot);
  const config = { schema: 'interface-v1', contracts: [contract] };
  write('contracts.json', config);
  const git = (...args) => execFileSync('git', ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test', ...args], { cwd: root, encoding: 'utf8', stdio: 'pipe' }).trim();
  return { root, contract, snapshot, config, write, git, check: extra => checkContracts({ root, manifest: 'contracts.json', ...extra }) };
}

test('reviewed public declarations pass, while changed and missing exports fail', t => {
  const f = fixture(t);
  assert.equal(f.check().status, 'pass');
  f.write('src/document.rs', '/// New docs\npub fn read( source : &str ) -> usize { 42 }');
  assert.equal(f.check().status, 'pass', 'implementation and formatting are outside the snapshot');
  f.write('src/document.rs', 'pub fn read(source: &[u8]) -> usize { source.len() }');
  assert.equal(f.check().findings[0].code, 'snapshot.mismatch');
  f.write('src/document.rs', 'fn read(source: &str) -> usize { source.len() }');
  assert.equal(f.check().status, 'fail');
});

test('an unchanged facade cannot hide changed declarations behind its reexports', t => {
  const f = fixture(t);
  f.contract.declarations = [{ source: 'src/implementation.rs', items: ['read'] }];
  f.snapshot.surface.declarations = f.snapshot.surface.exports;
  f.snapshot.surface.exports = [{ name: 'read', kind: 'use', declaration: 'pub use crate : : implementation : : read ;' }];
  f.write('src/document.rs', 'pub use crate::implementation::read;');
  f.write('src/implementation.rs', 'pub fn read(source: &str) -> usize { source.len() }');
  f.write('contracts.json', f.config); f.write(f.contract.snapshot, f.snapshot);
  assert.equal(f.check().status, 'pass');
  f.write('src/implementation.rs', 'pub fn read(source: &[u8]) -> usize { source.len() }');
  const changed = f.check();
  assert.equal(changed.status, 'fail');
  assert.equal(changed.findings[0].code, 'snapshot.mismatch');
  f.write('src/implementation.rs', 'fn read(source: &str) -> usize { source.len() }');
  assert.match(f.check().findings[0].message, /Registered public item read is missing/);
});

test('a facade must remain public and linked to the registered source in its crate', t => {
  const f = fixture(t);
  assert.equal(f.check().status, 'pass');
  for (const declaration of ['mod document;', '', 'pub mod document {}', '#[path="other.rs"] pub mod document;']) {
    f.write('src/lib.rs', declaration);
    const report = f.check();
    assert.equal(report.status, 'fail', `unavailable facade: ${declaration}`);
    assert.equal(report.findings[0].code, 'contract.invalid');
  }
  f.write('src/lib.rs', 'pub mod document;');
  f.write('src/unused.rs', 'pub fn read(source: &str) -> usize { source.len() }');
  f.contract.source = 'src/unused.rs';
  f.write('contracts.json', f.config);
  assert.match(f.check().findings[0].message, /does not load the registered facade source/);
});

test('updating a reviewed snapshot does not hide the interface change from the Git base', t => {
  const f = fixture(t);
  f.git('init', '-q'); f.git('add', '.'); f.git('commit', '-qm', 'original interface');
  const base = f.git('rev-parse', 'HEAD');
  f.write('src/document.rs', 'pub fn read(source: &[u8]) -> usize { source.len() }');
  const next = structuredClone(f.snapshot);
  next.surface.exports[0].declaration = 'pub fn read ( source : & [ u8 ] ) - > usize';
  f.write('snapshots/core.json', next);
  const report = f.check({ base });
  assert.equal(report.status, 'pass');
  assert.deepEqual(report.changes, [{ contract: 'core.document', producer: 'richview-core', consumers: ['richview-app'], change: 'changed' }]);
  assert.equal(f.check({ base: 'not-a-real-ref' }).status, 'fail');
});

test('removing a registered interface is reported even after its current snapshot is gone', t => {
  const f = fixture(t);
  const second = { ...f.contract, id: 'chart.api', producer: 'richview-chart', module: 'richview_chart::api', source: 'chart/api.rs', crateRoot: 'chart/lib.rs', snapshot: 'snapshots/chart.json', consumers: ['richview-core'] };
  const baseline = { ...f.snapshot, id: second.id, producer: second.producer, surface: { module: second.module, registration: 'pub mod api ;', exports: [{ name: 'layout', kind: 'fn', declaration: 'pub fn layout ( width : f64 ) - > f64' }], declarations: [] } };
  f.write('chart/lib.rs', 'pub mod api;');
  f.write('chart/api.rs', 'pub fn layout(width: f64) -> f64 { width }'); f.write(second.snapshot, baseline);
  f.write('contracts.json', { ...f.config, contracts: [f.contract, second] });
  f.git('init', '-q'); f.git('add', '.'); f.git('commit', '-qm', 'two interfaces');
  const base = f.git('rev-parse', 'HEAD');
  f.write('contracts.json', f.config); unlinkSync(join(f.root, second.snapshot)); unlinkSync(join(f.root, second.source));
  const report = f.check({ base });
  assert.equal(report.status, 'pass');
  assert.deepEqual(report.changes, [{ contract: 'chart.api', producer: 'richview-chart', consumers: ['richview-core'], change: 'removed' }]);
});

test('empty/missing manifests and snapshots fail closed', t => {
  const f = fixture(t);
  unlinkSync(join(f.root, f.contract.snapshot));
  assert.equal(f.check().status, 'fail');
  f.write('contracts.json', { schema: 'interface-v1', contracts: [] });
  assert.equal(f.check().status, 'fail');
  unlinkSync(join(f.root, 'contracts.json'));
  assert.equal(f.check().status, 'fail');
});

test('entry contracts emit observed deduplicated dependency triples and fail incomplete endpoints', t => {
  const f = fixture(t);
  const contract = { id: 'runtime.report.loader', producer: 'richview-runtime', kind: 'entries', entries: ['/report-runtime.js'], snapshot: 'snapshots/runtime.json', bindings: [
    { consumer: 'richview-build', source: 'src/names.rs', entry: '/report-runtime.js' },
    { consumer: 'richview-editor', source: 'src/editor.cjs', entry: '/report-runtime.js' },
    { consumer: 'richview-editor', source: 'src/download.cjs', entry: '/report-runtime.js' },
  ] };
  f.write('src/names.rs', 'pub const LOADER: &str = "/report-runtime.js";');
  f.write('src/editor.cjs', 'importScripts("/report-runtime.js");');
  f.write('src/download.cjs', 'exports.loader = "/report-runtime.js";');
  f.write(contract.snapshot, { schema: 'interface-v1', id: contract.id, producer: contract.producer, kind: 'entries', surface: { entries: ['/report-runtime.js'], bindings: [contract.bindings[0], contract.bindings[2], contract.bindings[1]] } });
  f.write('contracts.json', { ...f.config, contracts: [f.contract, contract] });
  const report = f.check();
  assert.equal(report.status, 'pass', JSON.stringify(report));
  assert.deepEqual(report.indirect, [
    { consumer: 'richview-build', producer: 'richview-runtime', contract: 'runtime.report.loader' },
    { consumer: 'richview-editor', producer: 'richview-runtime', contract: 'runtime.report.loader' },
  ]);
  unlinkSync(join(f.root, 'src/editor.cjs'));
  const missing = f.check();
  assert.equal(missing.status, 'fail');
  assert.deepEqual(missing.indirect, []);
  assert.deepEqual(missing.checked, ['core.document']);
});

test('CLI snapshot preparation is reviewable stdout and never overwrites the reviewed file', t => {
  const f = fixture(t);
  const script = fileURLToPath(new URL('../check.mjs', import.meta.url));
  const original = readFileSync(join(f.root, f.contract.snapshot), 'utf8');
  const result = spawnSync(process.execPath, [script, 'snapshot', '--root', f.root, '--manifest', 'contracts.json'], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  const { path, snapshot } = JSON.parse(result.stdout).snapshots[0];
  assert.equal(path, f.contract.snapshot); assert.deepEqual(snapshot, f.snapshot);
  const one = spawnSync(process.execPath, [script, 'snapshot', '--root', f.root, '--manifest', 'contracts.json', '--contract', f.contract.id], { encoding: 'utf8' });
  assert.equal(one.status, 0, one.stderr); assert.deepEqual(JSON.parse(one.stdout), f.snapshot);
  assert.equal(readFileSync(join(f.root, f.contract.snapshot), 'utf8'), original);
  const bad = spawnSync(process.execPath, [script, 'check', '--unknown', 'value'], { encoding: 'utf8' });
  assert.notEqual(bad.status, 0); assert.equal(JSON.parse(bad.stdout).status, 'fail');
  const partial = spawnSync(process.execPath, [script, 'check', '--root', f.root, '--manifest', 'contracts.json', '--contract', f.contract.id], { encoding: 'utf8' });
  assert.notEqual(partial.status, 0); assert.equal(JSON.parse(partial.stdout).status, 'fail');
});
