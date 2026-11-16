// Host reason (ARC-03): build tooling that writes the third-party notices the bundled VS Code extension
// must carry (ARC-68); it ships nothing itself.
//
// Usage, from packages/vscode: node ../../scripts/vscode-notices.cjs <esbuild metafile> <notices file>
// The metafile lists every input of dist/extension.js; the script writes one section per npm package
// among them, each the package's own licence file unchanged, then deletes the metafile.
const fs = require('node:fs');
const path = require('node:path');

const LICENCE_FILE = /^(?:licen[cs]e|copying)(?:[.-].*)?$/i;

/** The directory of the npm package an input file belongs to, or null for a file outside node_modules. */
function packageRoot(input) {
  const parts = input.split(/[\\/]/);
  const at = parts.lastIndexOf('node_modules');
  if (at < 0 || at + 1 >= parts.length) return null;
  const end = parts[at + 1].startsWith('@') ? at + 3 : at + 2;
  return parts.slice(0, end).join('/');
}

/** One { name, version, license, repository, text } per npm package that esbuild bundled, by name. */
function bundledPackages(metafile, cwd) {
  const roots = new Set();
  for (const input of Object.keys(metafile.inputs)) {
    const root = packageRoot(input);
    if (root) roots.add(root);
  }
  return [...roots].map((root) => {
    const directory = path.resolve(cwd, root);
    const manifest = JSON.parse(fs.readFileSync(path.join(directory, 'package.json'), 'utf8'));
    const file = fs.readdirSync(directory).filter((name) => LICENCE_FILE.test(name)).sort()[0];
    if (!file) throw new Error(`${manifest.name} ${manifest.version} is bundled but has no licence file`);
    const license = typeof manifest.license === 'string' ? manifest.license : manifest.license?.type;
    const repository = typeof manifest.repository === 'string' ? manifest.repository : manifest.repository?.url;
    return {
      name: manifest.name, version: manifest.version, license: license ?? 'see the licence text', repository,
      text: fs.readFileSync(path.join(directory, file), 'utf8').trim(),
    };
  }).sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

/** The notices file for the packages that a metafile names. */
function notices(metafile, cwd) {
  const rule = '-'.repeat(80);
  const sections = bundledPackages(metafile, cwd).map((pkg) => [
    rule, `${pkg.name} ${pkg.version} (${pkg.license})`, ...(pkg.repository ? [pkg.repository.replace(/^git\+/, '')] : []), rule, '', pkg.text, '',
  ].join('\n'));
  return [
    'Third-party notices for the RichView VS Code extension',
    '',
    'dist/extension.js bundles the npm packages below. Each section is the package\'s own licence file, unchanged.',
    '',
    ...sections,
  ].join('\n');
}

module.exports = { bundledPackages, notices, packageRoot };

if (require.main === module) {
  const [metafile, output] = process.argv.slice(2);
  if (!metafile || !output) throw new Error('Usage: vscode-notices.cjs <esbuild metafile> <notices file>');
  fs.writeFileSync(output, notices(JSON.parse(fs.readFileSync(metafile, 'utf8')), process.cwd()));
  fs.rmSync(metafile);
}
