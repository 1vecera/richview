// Host reason (ARC-03): generate the notices delivered with native binaries and the Wasm site.
// `--write` updates the checked-in hash manifest; `--check` refuses a stale or incomplete one.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, realpathSync, lstatSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(import.meta.url);
const { bundledPackages } = require('./vscode-notices.cjs');
// The Windows preview (ARC-67) is listed so one notice file covers every native archive.
const targets = ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc', 'wasm32-unknown-unknown'];
const manifestFile = join(root, 'scripts/third-party-notices-manifest.json');
const sources = join(root, 'scripts/notices-sources');
const cratesIo = 'registry+https://github.com/rust-lang/crates.io-index';
// Published crate archives missing their own licence file. Upstream entries are pinned to the
// crate's .cargo_vcs_info.json; SPDX is used only where that exact upstream tree lacks a text.
const fallback = {
  'alloc-stdlib@0.3.0': { file: 'alloc.txt', revision: '0a81fd6928ea3b33c8cd484aa4575d50ffb98012', origin: 'upstream', path: 'LICENSE' },
  'defmt-parser@1.0.0': { file: 'defmt.txt', revision: '4a8cdb44891ed57b8ff5a023b6bec7137c48708f', origin: 'upstream', path: 'LICENSE-APACHE' },
  'duckdb@1.10506.1': { file: 'duckdb.txt', revision: '2d25cb2a4e6c477e9d48db9b22fa67d80c859b1e', origin: 'upstream', path: 'LICENSE' },
  'instant-xml@0.7.6': { file: 'spdx-apache.txt', revision: '63bf1953fe7b7838770ba4e8d11651c229960523', origin: 'SPDX' },
  'instant-xml-macros@0.7.2': { file: 'spdx-apache.txt', revision: 'd3101874c2ba1b4081d581590d43736de9b5e67c', origin: 'SPDX' },
};
const licenseName = /^(?:(?:licen[cs]e|copying|notice|unlicense)(?:[._-].*)?|(?:mit|apache|bsd)[-_]licen[cs]e(?:[._-].*)?)$/i;
const mplNames = new Set(['cssparser', 'dtoa-short']);
// libduckdb-sys bundles C/C++ sources which Cargo's dependency graph does not enumerate.
// These notices come from the exact upstream DuckDB source revision; the crate's archive is
// pinned below so an update cannot silently drop or change one of these libraries.
const duckdbRevision = '069cc9f9b5be802405797faecc284961b07c70ef';
const duckdbArchiveHash = '05c43055c3c76c630957e787b160925f9212f33265f4e7dbbc413bdf655b5ddf';
// These three upstream notices contain public author addresses. Fetch them at the exact
// DuckDB revision into ignored scratch and verify their hashes instead of committing them.
const remoteDuckdb = {
  'pdqsort__LICENSE': '323fc18e35fcf058d69d684022ef844f206cb8fefb2bac1298fa1767e0b9ae38',
  'vergesort__LICENSE': '12dd392860ea2a3633bce5f33d7eebd4308a7e3386b4372260e5e72be7a64033',
  'yyjson__LICENSE': '45e384d3d52c73cba3a64d6e6c25d47cd738cd8a55c30629e3201046eda62947',
};
const duckdbNotices = [
  'DUCKDB-LICENSE', 'brotli__LICENSE', 'concurrentqueue__LICENSE', 'fast_float__LICENSE',
  'fastpforlib__LICENSE', 'fmt__LICENSE', 'fsst__LICENSE', 'httplib__LICENSE',
  'hyperloglog__LICENSE', 'jaro_winkler__LICENSE', 'libpg_query__LICENSE', 'lz4__LICENSE',
  'mbedtls__LICENSE', 'miniz__LICENSE', 'parquet__LICENSE', 'pcg__LICENSE', 'pdqsort__LICENSE',
  're2__LICENSE', 'ska_sort__LICENSE', 'skiplist__LICENSE', 'snappy__LICENSE', 'tdigest__LICENSE',
  'tdigest__NOTICES', 'thrift__thrift__LICENSE', 'utf8proc__LICENSE', 'vergesort__LICENSE',
  'yyjson__LICENSE', 'zstd__LICENSE',
];
const digest = text => createHash('sha256').update(text).digest('hex');
function insist(ok, message) { if (!ok) throw new Error(message); }
function regular(file) {
  const info = lstatSync(file);
  insist(info.isFile() && info.size > 0 && info.size < 2 * 1024 * 1024, `missing or invalid notice text: ${file}`);
  return new TextDecoder('utf-8', { fatal: true }).decode(readFileSync(file)).replace(/\r\n/g, '\n').replace(/[\t ]+$/gm, '').trimEnd() + '\n';
}
function within(base, file) {
  const path = relative(realpathSync(base), realpathSync(file));
  return path !== '..' && !path.startsWith(`..${sep}`) && !isAbsolute(path);
}

// The two known MPL crates must remain unmodified crates.io packages. Reject [patch] entries
// naming them, including renamed packages, even if a patch is currently unused.
export function checkMplPatches(text, name, names = mplNames) {
  let section = '';
  for (const line of text.split(/\r?\n/)) {
    const header = /^\s*\[([^\]]+)\]/.exec(line);
    if (header) {
      section = header[1].replace(/['"]/g, '');
      if (/^patch\./.test(section) && [...names].some(item => section.split('.').includes(item))) throw new Error(`MPL patch is not allowed in ${name}: ${section}`);
      continue;
    }
    if (!/^patch\./.test(section)) continue;
    const uncommented = line.replace(/\s+#.*$/, '');
    if ([...names].some(item => new RegExp(`^\\s*["']?${item}["']?\\s*=|\\bpackage\\s*=\\s*["']${item}["']`).test(uncommented))) throw new Error(`MPL patch is not allowed in ${name}`);
  }
}
function verifyPatches(packages) {
  const names = new Set([...mplNames, ...packages.filter(pkg => pkg.license?.includes('MPL-2.0')).map(pkg => pkg.name)]);
  const configs = [join(root, 'Cargo.toml')];
  for (let dir = root; ; dir = dirname(dir)) {
    configs.push(join(dir, '.cargo/config'), join(dir, '.cargo/config.toml'));
    if (dirname(dir) === dir) break;
  }
  const cargoHome = process.env.CARGO_HOME || join(process.env.HOME, '.cargo');
  configs.push(join(cargoHome, 'config'), join(cargoHome, 'config.toml'));
  for (const file of new Set(configs)) {
    if (existsSync(file)) checkMplPatches(readFileSync(file, 'utf8'), file, names);
  }
}
function metadata(target) {
  return JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--format-version', '1', '--filter-platform', target], { cwd: root, encoding: 'utf8', maxBuffer: 128 * 1024 * 1024 }));
}
function editorPackages() {
  const editor = join(root, 'packages/vscode'), scratch = join(root, 'tmp');
  const esbuild = join(root, 'node_modules/esbuild/bin/esbuild');
  insist(existsSync(esbuild), 'editor bundle dependencies are missing; run npm ci --ignore-scripts');
  mkdirSync(scratch, { recursive: true });
  const dir = mkdtempSync(join(scratch, 'notices-bundle-'));
  try {
    const meta = join(dir, 'meta.json');
    execFileSync(process.execPath, [esbuild, 'extension.cjs', '--bundle', '--platform=node', '--format=cjs', '--external:vscode', `--outfile=${join(dir, 'extension.js')}`, `--metafile=${meta}`], { cwd: editor, stdio: 'pipe' });
    return bundledPackages(JSON.parse(readFileSync(meta, 'utf8')), editor);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}
function licenseFiles(dir) {
  const files = [];
  function walk(folder) {
    for (const entry of readdirSync(folder, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name, 'en'))) {
      const path = join(folder, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile() && licenseName.test(entry.name)) files.push(path);
    }
  }
  walk(dir);
  return files;
}
export function collect(metadatas) {
  const packages = new Map();
  for (const meta of metadatas) {
    const workspace = new Set(meta.workspace_members), resolved = new Set(meta.resolve.nodes.map(node => node.id));
    for (const pkg of meta.packages) {
      if (!resolved.has(pkg.id)) continue;
      if (workspace.has(pkg.id)) {
        insist(!mplNames.has(pkg.name) && !pkg.license?.includes('MPL-2.0'), `MPL crate is patched, vendored or non-registry: ${pkg.name}@${pkg.version}`);
      } else packages.set(pkg.id, pkg);
    }
  }
  return [...packages.values()].sort((a, b) => a.name.localeCompare(b.name, 'en') || a.version.localeCompare(b.version, 'en') || String(a.source).localeCompare(String(b.source), 'en'));
}
export function generate(packages, npmPackages = [], downloaded = {}) {
  const texts = new Map(), entries = [], missing = [];
  const cargoHome = process.env.CARGO_HOME || join(process.env.HOME, '.cargo');
  const registry = join(cargoHome, 'registry/src');
  for (const pkg of packages) {
    const id = `${pkg.name}@${pkg.version}`, dir = dirname(pkg.manifest_path);
    insist(typeof pkg.license === 'string' && pkg.license.trim(), `missing licence expression: ${id}`);
    if (mplNames.has(pkg.name) || pkg.license.includes('MPL-2.0')) {
      insist(pkg.source === cratesIo && existsSync(registry) && within(registry, dir), `MPL crate is patched, vendored or non-registry: ${id}`);
      insist(pkg.repository, `MPL crate has no upstream repository: ${id}`);
    }
    let files = licenseFiles(dir);
    if (pkg.license_file) {
      const file = isAbsolute(pkg.license_file) ? pkg.license_file : join(dir, pkg.license_file);
      insist(within(dir, file), `licence path escapes crate: ${id}`);
      if (!files.includes(file)) files.push(file);
    }
    let origin = 'crate archive';
    if (!files.length && fallback[id]) {
      const entry = fallback[id];
      const vcs = JSON.parse(readFileSync(join(dir, '.cargo_vcs_info.json'), 'utf8'));
      insist(vcs.git?.sha1 === entry.revision && pkg.source === cratesIo, `fallback source revision differs: ${id}`);
      files = [join(sources, entry.file)];
      origin = entry.origin === 'upstream' ? `exact upstream commit ${entry.revision}: ${pkg.repository}/blob/${entry.revision}/${entry.path}` : 'canonical SPDX Apache-2.0 text (https://spdx.org/licenses/Apache-2.0.html); crate and exact upstream tree omit a text';
    }
    if (!files.length) { missing.push(id); continue; }
    const refs = [];
    for (const file of files) {
      const text = regular(file), key = digest(text);
      if (!texts.has(key)) texts.set(key, text);
      if (!refs.includes(key)) refs.push(key);
    }
    entries.push({ pkg, origin, refs });
  }
  insist(!missing.length, `missing licence or notice text: ${missing.join(', ')}`);
  const duckdb = packages.find(pkg => pkg.name === 'libduckdb-sys');
  insist(duckdb?.version === '1.10506.1' && duckdb.source === cratesIo, 'DuckDB source changed; review its bundled third-party notices');
  const archive = join(dirname(duckdb.manifest_path), 'duckdb.tar.gz');
  insist(digest(readFileSync(archive)) === duckdbArchiveHash, 'DuckDB bundled sources differ; review their third-party notices');
  const embedded = duckdbNotices.map(name => {
    const text = downloaded[name] ?? regular(join(sources, 'duckdb-third-party', name)), key = digest(text);
    if (!texts.has(key)) texts.set(key, text);
    return { name, key };
  });
  const boostText = regular(join(sources, 'spdx-bsl.txt')), boostKey = digest(boostText);
  if (!texts.has(boostKey)) texts.set(boostKey, boostText);
  const npm = npmPackages.map(pkg => {
    insist(pkg.text?.trim(), `missing bundled npm licence text: ${pkg.name}@${pkg.version}`);
    const text = pkg.text.replace(/\r\n/g, '\n').replace(/[\t ]+$/gm, '').trimEnd() + '\n', key = digest(text);
    if (!texts.has(key)) texts.set(key, text);
    return { ...pkg, key };
  });
  const ids = new Map([...texts.keys()].map((key, index) => [key, `T${String(index + 1).padStart(3, '0')}`]));
  const lines = [
    'RichView third-party notices', '',
    'This file covers the resolved Rust dependency graph for the macOS arm64, Linux x64, Windows x64 and WebAssembly targets checked by the dependency gate. It includes build and development dependencies as a conservative superset.',
    'Each entry refers to one or more original licence or notice texts below; identical texts are printed once.', '',
    'CRATES', '======', '',
  ];
  for (const { pkg, origin, refs } of entries) {
    lines.push(`${pkg.name} ${pkg.version}`, `Licence: ${pkg.license}`, `Authors: ${pkg.authors?.length ? pkg.authors.join('; ') : 'not specified in the crate manifest'}`, `Repository: ${pkg.repository || 'not specified in the crate manifest'}`, `Text source: ${origin}.`, `Texts: ${refs.map(ref => ids.get(ref)).join(', ')}`);
    if (pkg.license.includes('MPL-2.0')) lines.push(`RichView uses this crate unmodified. Source: https://crates.io/api/v1/crates/${encodeURIComponent(pkg.name)}/${encodeURIComponent(pkg.version)}/download ; upstream: ${pkg.repository}`);
    if (pkg.name === 'webpki-roots') lines.push('The root-certificate data derives from the Common CA Database (CCADB), as published by Mozilla. CCADB Data Usage Terms: https://www.ccadb.org/rootstores/usage#ccadb-data-usage-terms');
    lines.push('');
  }
  lines.push('BUNDLED DUCKDB NATIVE SOURCES', '=============================', '',
    `libduckdb-sys ${duckdb.version} bundles DuckDB and native third-party code not enumerated as Rust crates. These notices come from DuckDB v1.5.6 at ${duckdbRevision}; the crate's bundled source archive is checked before generation.`, '');
  for (const { name, key } of embedded) {
    const upstream = name === 'DUCKDB-LICENSE' ? 'LICENSE' : `third_party/${name.replaceAll('__', '/')}`;
    lines.push(`${name.replaceAll('__', '/')}`, `Source: https://github.com/duckdb/duckdb/blob/${duckdbRevision}/${upstream}`, `Text: ${ids.get(key)}`);
    if (name === 'mbedtls__LICENSE') lines.push('Mbed TLS is offered under Apache-2.0 OR GPL-2.0-or-later; RichView uses the Apache-2.0 option.');
    if (name === 'ska_sort__LICENSE') lines.push(`The upstream notice names Boost Software License 1.0; its canonical SPDX text is ${ids.get(boostKey)}: https://spdx.org/licenses/BSL-1.0.html`);
    lines.push('');
  }
  lines.push('BUNDLED VS CODE EXTENSION PACKAGES', '==================================', '',
    'Only npm packages actually included by the extension bundler are listed. The website’s handwritten JavaScript imports no npm packages.', '');
  for (const pkg of npm) lines.push(`${pkg.name} ${pkg.version}`, `Licence: ${pkg.license}`, `Repository: ${pkg.repository || 'not specified'}`, `Text: ${ids.get(pkg.key)}`, '');
  lines.push('LICENCE AND NOTICE TEXTS', '========================', '');
  for (const [key, text] of texts) lines.push(`[${ids.get(key)}]`, text.trimEnd(), '');
  const manifest = {
    notice_sha256: digest(lines.join('\n')),
    crates: entries.map(({ pkg, refs }) => ({ name: pkg.name, version: pkg.version, licence: pkg.license, texts: refs })),
    duckdb: embedded.map(({ name, key }) => ({ name, text: key })),
    boost: boostKey,
    npm: npm.map(pkg => ({ name: pkg.name, version: pkg.version, licence: pkg.license, text: pkg.key })),
  };
  return { text: lines.join('\n'), manifest: JSON.stringify(manifest, null, 2) + '\n', crates: entries.length, npm: npm.length, texts: texts.size };
}
async function upstreamNotices() {
  const cache = join(root, 'tmp/notices-sources');
  mkdirSync(cache, { recursive: true });
  const result = {};
  for (const [name, expected] of Object.entries(remoteDuckdb)) {
    const file = join(cache, name);
    if (!existsSync(file) || digest(regular(file)) !== expected) {
      const path = name.replaceAll('__', '/');
      const url = `https://raw.githubusercontent.com/duckdb/duckdb/${duckdbRevision}/third_party/${path}`;
      const response = await fetch(url, { signal: AbortSignal.timeout(30000) });
      insist(response.ok, `cannot fetch exact upstream notice: ${name} (HTTP ${response.status})`);
      const text = (await response.text()).replace(/\r\n/g, '\n').replace(/[\t ]+$/gm, '').trimEnd() + '\n';
      insist(digest(text) === expected, `upstream notice differs from pinned hash: ${name}`);
      writeFileSync(file, text);
    }
    result[name] = regular(file);
    insist(digest(result[name]) === expected, `cached upstream notice differs from pinned hash: ${name}`);
  }
  return result;
}
async function main(args) {
  insist(args.length === 0 || (args.length === 1 && ['--check', '--write'].includes(args[0])) || (args.length === 2 && args[0] === '--generate'), 'usage: node scripts/third-party-notices.mjs [--write|--check|--generate FILE]');
  const packages = collect(targets.map(metadata));
  verifyPatches(packages);
  const result = generate(packages, editorPackages(), await upstreamNotices());
  if (args[0] === '--write') writeFileSync(manifestFile, result.manifest);
  else insist(existsSync(manifestFile) && readFileSync(manifestFile, 'utf8') === result.manifest, 'third-party notices manifest is missing or stale; run node scripts/third-party-notices.mjs --write');
  if (args[0] === '--generate') { mkdirSync(dirname(resolve(args[1])), { recursive: true }); writeFileSync(resolve(args[1]), result.text); }
  console.log(`${result.crates} crates, ${result.npm} bundled npm packages, ${result.texts} distinct licence/notice texts; missing 0`);
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main(process.argv.slice(2)).catch(error => { console.error(`third-party notices: ${error.message}`); process.exitCode = 1; });
}
