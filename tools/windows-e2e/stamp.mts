// Stamps install.ps1 for the end-to-end walk of one archive, served at BASE (loopback HTTP), so the
// walk can install with the one-liner `irm BASE/install.ps1 | iex` exactly as a user does.
// A release is stamped by scripts/assemble-release.ts, never by this file.
// Usage: node stamp.mts <install.ps1 template> <archive> <full commit> <base URL> <output install.ps1>
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
const [template, archive, commit, base, output] = process.argv.slice(2);
if (!/^[0-9a-f]{40}$/.test(commit ?? '') || !/^http:\/\/127\.0\.0\.1:\d+$/.test(base ?? '') || !output) throw new Error('usage: stamp.mts TEMPLATE ARCHIVE COMMIT http://127.0.0.1:PORT OUTPUT');
const stamp = { release_commit: commit, release_base_url: base, release_windows_sha256: createHash('sha256').update(readFileSync(archive)).digest('hex') };
let text = readFileSync(template, 'utf8');
for (const [field, value] of Object.entries(stamp)) {
  if (!text.includes(`$${field}='UNSTAMPED'`)) throw new Error(`template has no unstamped ${field}`);
  text = text.replace(`$${field}='UNSTAMPED'`, () => `$${field}='${value}'`);
}
writeFileSync(output, text);
console.log(`${stamp.release_windows_sha256}  ${archive}`);
