// Third-party notices of the bundled extension (ARC-68): one section per bundled npm package, licence text unchanged.
import type { TestContext } from 'node:test';
const assert: typeof import('node:assert/strict') = require('node:assert/strict');
const fs: typeof import('node:fs') = require('node:fs');
const os: typeof import('node:os') = require('node:os');
const path: typeof import('node:path') = require('node:path');
const test: typeof import('node:test').test = require('node:test');
const { bundledPackages, notices, packageRoot }: typeof import('../../../scripts/vscode-notices.ts') = require('../../../scripts/vscode-notices.ts');

function project(t: TestContext, packages: Record<string, { manifest: object; files: Record<string, string> }>) {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'rv-notices-'));
  t.after(() => fs.rmSync(cwd, { recursive: true, force: true }));
  for (const [directory, { manifest, files }] of Object.entries(packages)) {
    fs.mkdirSync(path.join(cwd, directory), { recursive: true });
    fs.writeFileSync(path.join(cwd, directory, 'package.json'), JSON.stringify(manifest));
    for (const [name, text] of Object.entries(files)) fs.writeFileSync(path.join(cwd, directory, name), text);
  }
  return cwd;
}

test('the package of an input is the last node_modules folder, scoped names included', () => {
  assert.equal(packageRoot('../../node_modules/minimatch/dist/commonjs/index.js'), '../../node_modules/minimatch');
  assert.equal(packageRoot('node_modules/@isaacs/glob-match/dist/index.js'), 'node_modules/@isaacs/glob-match');
  assert.equal(packageRoot('node_modules/a/node_modules/b/lib/x.js'), 'node_modules/a/node_modules/b');
  assert.equal(packageRoot('extension.cts'), null);
});

test('each bundled package gets its licence file unchanged, once, in name order', (t) => {
  const cwd = project(t, {
    'node_modules/minimatch': { manifest: { name: 'minimatch', version: '11.0.3', license: 'BlueOak-1.0.0', repository: { url: 'git+https://example.test/minimatch.git' } }, files: { 'LICENSE.md': '# Blue Oak Model License\n' } },
    'node_modules/@s/b': { manifest: { name: '@s/b', version: '1.0.0', license: 'MIT' }, files: { LICENSE: 'MIT text\n', 'README.md': 'not a licence' } },
  });
  const metafile = { inputs: { 'extension.cts': {}, 'node_modules/minimatch/a.js': {}, 'node_modules/minimatch/b.js': {}, 'node_modules/@s/b/x.js': {} } };
  assert.deepEqual(bundledPackages(metafile, cwd).map((pkg) => pkg.name), ['@s/b', 'minimatch']);
  const text = notices(metafile, cwd);
  assert.ok(text.includes('minimatch 11.0.3 (BlueOak-1.0.0)\nhttps://example.test/minimatch.git'));
  assert.ok(text.includes('# Blue Oak Model License'));
  assert.ok(text.includes('MIT text'));
  assert.ok(!text.includes('not a licence'));
  assert.equal(text.match(/minimatch 11\.0\.3/g)!.length, 1);
});

test('a bundled package without a licence file is an error', (t) => {
  const cwd = project(t, { 'node_modules/bare': { manifest: { name: 'bare', version: '1.0.0', license: 'MIT' }, files: { 'README.md': 'text' } } });
  assert.throws(() => bundledPackages({ inputs: { 'node_modules/bare/i.js': {} } }, cwd), /bare 1\.0\.0 is bundled but has no licence file/);
});
