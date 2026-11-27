// Host reason (ARC-03): exercise the shipped third-party notice generator's security checks.
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { checkMplPatches, collect, generate } from '../../../scripts/third-party-notices.ts';

test('MPL patches are rejected even when renamed or declared as a table', () => {
  for (const config of [
    '[patch.crates-io]\ncssparser = { path = "patched" }\n',
    '[patch.crates-io]\nrenamed = { package = "dtoa-short", git = "https://example.test" }\n',
    '[patch.crates-io.cssparser]\nversion = "1"\n',
  ]) assert.throws(() => checkMplPatches(config, 'fixture'), /MPL patch/);
  assert.doesNotThrow(() => checkMplPatches('[patch.crates-io]\nother = { path = "elsewhere" }\n', 'fixture'));
});

test('the target union keeps non-workspace dependencies only once', () => {
  const pkg = { id: 'registry#a', name: 'a', version: '1', source: 'registry' };
  const workspace = { id: 'workspace#app', name: 'app', version: '1', source: null };
  const meta = { workspace_members: [workspace.id], resolve: { nodes: [{ id: pkg.id }, { id: workspace.id }] }, packages: [pkg, workspace] };
  assert.deepEqual(collect([meta, meta]), [pkg]);
  assert.throws(() => collect([{ ...meta, packages: [pkg, { ...workspace, name: 'cssparser', license: 'MPL-2.0' }] }]), /patched, vendored or non-registry/);
});

test('a crate without licence text and a non-registry MPL crate both fail closed', t => {
  mkdirSync('tmp', { recursive: true });
  const dir = mkdtempSync(resolve('tmp/notices-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const basic = { name: 'missing', version: '1', license: 'MIT', manifest_path: join(dir, 'Cargo.toml'), source: 'registry', authors: [] };
  writeFileSync(basic.manifest_path, '');
  assert.throws(() => generate([basic]), /missing licence or notice text: missing@1/);
  assert.throws(() => generate([{ ...basic, name: 'cssparser', license: 'MPL-2.0', source: null }]), /patched, vendored or non-registry/);
});
