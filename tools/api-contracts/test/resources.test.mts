// Host reason (ARC-03): real local files prove include/entry bindings and resource identity.
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, renameSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import type { TestContext } from 'node:test';
import { entrySurface, repositoryFile, resourceSurface } from '../lib/resources.mts';

function fixture(t: TestContext) {
  mkdirSync('tmp', { recursive: true });
  const dir = mkdtempSync(join(process.cwd(), 'tmp/api-resource-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const path of ['producer', 'consumer']) mkdirSync(join(dir, path));
  writeFileSync(join(dir, 'producer/report.css'), 'body { color: black; }\n');
  writeFileSync(join(dir, 'consumer/lib.rs'), 'const STYLE: &str = include_str!("../producer/report.css");\n');
  return {
    dir,
    contract: { id: 'report.style', producer: 'richview-core', source: 'producer/report.css', bindings: [{ consumer: 'richview-editor', source: 'consumer/lib.rs', include: { macro: 'include_str', path: '../producer/report.css' } }] },
  };
}

test('a literal include identifies its producer and a resource edit changes the snapshot', t => {
  const { dir, contract } = fixture(t);
  const before = resourceSurface(dir, contract);
  assert.deepEqual(before.indirect, [{ consumer: 'richview-editor', producer: 'richview-core', contract: 'report.style' }]);
  writeFileSync(join(dir, contract.source), 'body { color: blue; }\n');
  assert.notEqual(resourceSurface(dir, contract).surface.sha256, before.surface.sha256);
  renameSync(join(dir, contract.source), join(dir, 'producer/renamed.css'));
  assert.throws(() => resourceSurface(dir, contract), /ENOENT/);
});

test('a stale include or different same-byte producer is refused', t => {
  const { dir, contract } = fixture(t);
  writeFileSync(join(dir, 'producer/other.css'), 'body { color: black; }\n');
  const other = structuredClone(contract); other.source = 'producer/other.css';
  assert.throws(() => resourceSurface(dir, other), /different producer/);
  writeFileSync(join(dir, 'consumer/lib.rs'), '// include_str!("../producer/report.css")\nconst STYLE: &str = include_str!("../producer/other.css");');
  assert.throws(() => resourceSurface(dir, contract), /does not include/);
});

test('runtime entry names are actual JavaScript strings, not comments, regexes or dynamic text', t => {
  const { dir } = fixture(t);
  writeFileSync(join(dir, 'producer/report.js'), 'export function start() { return "loaded"; }\n');
  const contract = { id: 'report.runtime.entry', producer: 'richview-runtime', source: 'producer/report.js', bindings: [{ consumer: 'richview-editor', source: 'consumer/loader.cjs', entry: '/report-runtime.js' }] };
  writeFileSync(join(dir, 'consumer/loader.cjs'), "importScripts('/report-runtime.js');\n");
  assert.equal(resourceSurface(dir, contract).indirect[0].contract, 'report.runtime.entry');
  const regex = String.raw`const decoy = /"\/report-runtime.js"/;`;
  writeFileSync(join(dir, 'consumer/loader.cjs'), '// "/report-runtime.js"\n' + regex + ' importScripts(`/changed-${name}.js`);');
  assert.throws(() => resourceSurface(dir, contract), /does not name runtime entry/);
});

test('a registered source cannot escape through a relative path or symlink', t => {
  const { dir } = fixture(t);
  assert.throws(() => repositoryFile(dir, '../outside.css'), /repository-relative/);
  const container = join(dir, 'inner'); mkdirSync(container);
  symlinkSync(join(dir, 'producer/report.css'), join(container, 'escape.css'));
  assert.throws(() => repositoryFile(container, 'escape.css'), /escapes/);
});

test('runtime names fail on either stale endpoint without a staged runtime or substitute bytes', t => {
  const { dir } = fixture(t);
  writeFileSync(join(dir, 'producer/names.rs'), 'pub const LOADER: &str = "/report-runtime.js";');
  writeFileSync(join(dir, 'consumer/loader.cjs'), 'importScripts("/report-runtime.js");');
  const contract = { id: 'runtime.loader', producer: 'richview-runtime', entries: ['/report-runtime.js'], bindings: [
    { consumer: 'richview-build', source: 'producer/names.rs', entry: '/report-runtime.js' },
    { consumer: 'richview-editor', source: 'consumer/loader.cjs', entry: '/report-runtime.js' },
  ] };
  assert.equal(entrySurface(dir, contract).indirect.length, 2);
  writeFileSync(join(dir, 'producer/names.rs'), 'pub const LOADER: &str = "/renamed.js";');
  assert.throws(() => entrySurface(dir, contract), /does not name runtime entry/);
  writeFileSync(join(dir, 'producer/names.rs'), 'pub const LOADER: &str = "/report-runtime.js";');
  writeFileSync(join(dir, 'consumer/loader.cjs'), 'importScripts("/stale.js");');
  assert.throws(() => entrySurface(dir, contract), /does not name runtime entry/);
});
