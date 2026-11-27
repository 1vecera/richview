// Host reason (ARC-03): build tooling that writes the third-party notices the bundled VS Code extension
// must carry (ARC-68); it ships nothing itself.
//
// Usage, from packages/vscode: node ../../scripts/vscode-notices.ts <esbuild metafile> <notices file>
// The metafile lists every input of dist/extension.js; the script writes one section per npm package
// among them, each the package's own licence file unchanged, then deletes the metafile.
import fs from 'node:fs';
import path from 'node:path';

type Metafile = { inputs: Record<string, unknown> };

interface PackageManifest {
  name: string;
  version: string;
  license?: string | { type: string };
  repository?: string | { url: string };
}

const LICENCE_FILE = /^(?:licen[cs]e|copying)(?:[.-].*)?$/i;

/** The directory of the npm package an input file belongs to, or null for a file outside node_modules. */
export function packageRoot(input: string) {
  const parts = input.split(/[\\/]/);
  const at = parts.lastIndexOf('node_modules');
  if (at < 0 || at + 1 >= parts.length) return null;
  const end = parts[at + 1].startsWith('@') ? at + 3 : at + 2;
  return parts.slice(0, end).join('/');
}

/** One { name, version, license, repository, text } per npm package that esbuild bundled, by name. */
export function bundledPackages(metafile: Metafile, cwd: string) {
  const roots = new Set<string>();
  for (const input of Object.keys(metafile.inputs)) {
    const root = packageRoot(input);
    if (root) roots.add(root);
  }
  return [...roots].map((root) => {
    const directory = path.resolve(cwd, root);
    const manifest: PackageManifest = JSON.parse(fs.readFileSync(path.join(directory, 'package.json'), 'utf8'));
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
export function notices(metafile: Metafile, cwd: string) {
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

if (import.meta.main) {
  const [metafile, output] = process.argv.slice(2);
  if (!metafile || !output) throw new Error('Usage: vscode-notices.ts <esbuild metafile> <notices file>');
  fs.writeFileSync(output, notices(JSON.parse(fs.readFileSync(metafile, 'utf8')), process.cwd()));
  fs.rmSync(metafile);
}
