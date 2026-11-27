// Host reason (ARC-03): assembles native release files and verifies published GitHub asset bytes.
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { existsSync, lstatSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const platforms = ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu'];
const fields = ['release_commit', 'release_base_url', 'release_macos_sha256', 'release_linux_sha256'];
// The Windows preview (ARC-67) is optional: its archive has its own installer, install.ps1, whose
// stamps are PowerShell variables. Without it a release is assembled and verified as before.
const windowsPlatform = 'x86_64-pc-windows-msvc';
const windowsFields = ['release_commit', 'release_base_url', 'release_windows_sha256'];
const hash = (bytes: Buffer) => createHash('sha256').update(bytes).digest('hex');
const name = (platform: string) => `richview-${platform}.tar.gz`;
const insist: (ok: unknown, message: string) => asserts ok = (ok, message) => { if (!ok) throw new Error(message); };
const regular = (path: string) => { const stat = lstatSync(path); insist(stat.isFile() && stat.size <= 100 * 1024 * 1024, `not a regular release file under 100 MiB: ${path}`); return readFileSync(path); };
const commitOk = (value: string | undefined) => /^[0-9a-f]{40}$/.test(value ?? '');
function tagUrl(tag: string) {
  insist(typeof tag === 'string' && tag.length > 0 && tag.length <= 255, 'release tag is required');
  const checked = spawnSync('git', ['check-ref-format', `refs/tags/${tag}`], { encoding: 'utf8' });
  insist(checked.status === 0, 'invalid release tag');
  const encoded = encodeURIComponent(tag).replace(/[!'()*]/g, ch => `%${ch.charCodeAt(0).toString(16).toUpperCase()}`);
  return `https://github.com/1vecera/richview/releases/download/${encoded}`;
}

function identity(bytes: Buffer, commit: string, platform: string) {
  const result = spawnSync('tar', ['-xOzf', '-', './BUILD'], { input: bytes, maxBuffer: 4096, timeout: 30_000, encoding: 'utf8' });
  insist(result.status === 0 && result.stdout === `commit=${commit}\nplatform=${platform}\n`, `archive BUILD differs from approved commit/platform: ${platform}`);
  const notice = spawnSync('tar', ['-xOzf', '-', './notices/THIRD-PARTY-NOTICES.txt'], { input: bytes, maxBuffer: 8 * 1024 * 1024, timeout: 30_000 });
  insist(notice.status === 0 && notice.stdout.length > 0, `archive lacks third-party notices: ${platform}`);
  return notice.stdout;
}
// `sigil` is what precedes a stamp's name in the installer: nothing in sh, `$` in PowerShell.
function stamps(text: string, names = fields, sigil = '') {
  return Object.fromEntries(names.map(field => {
    const matches = [...text.matchAll(new RegExp(`^${sigil.replace('$', '\\$')}${field}='([^']*)'$`, 'gm'))];
    insist(matches.length === 1, `installer must contain one ${field}`);
    return [field, matches[0][1]];
  }));
}
function stamped(template: string, values: Record<string, string>, sigil = '') {
  const current = stamps(template, Object.keys(values), sigil);
  insist(Object.values(current).every(value => value === 'UNSTAMPED'), 'installer template must be unstamped');
  for (const [field, value] of Object.entries(values)) {
    template = template.replace(`${sigil}${field}='UNSTAMPED'`, () => `${sigil}${field}='${value}'`);
  }
  return template;
}
interface AssemblyOptions {
  commit: string; tag: string; macos: string; linux: string; windows?: string;
  output: string; template?: string; windowsTemplate?: string;
}
export function assemble({ commit, tag, macos, linux, windows, output, template = join(root, 'scripts/install.sh'), windowsTemplate = join(root, 'scripts/install.ps1') }: AssemblyOptions) {
  insist(commitOk(commit), 'approved commit must be a full lowercase Git commit');
  const releaseUrl = tagUrl(tag);
  const targets = windows === undefined ? platforms : [...platforms, windowsPlatform];
  const inputs = (windows === undefined ? [macos, linux] : [macos, linux, windows]).map(regular);
  const notices = inputs.map((bytes, index) => identity(bytes, commit, targets[index]));
  insist(notices.every(notice => notice.equals(notices[0])), 'native archives have different third-party notices');
  const digests = inputs.map(hash);
  const script = stamped(regular(template).toString('utf8'), { release_commit: commit, release_base_url: releaseUrl, release_macos_sha256: digests[0], release_linux_sha256: digests[1] });
  const windowsScript = windows === undefined ? null : stamped(regular(windowsTemplate).toString('utf8'), { release_commit: commit, release_base_url: releaseUrl, release_windows_sha256: digests[2] }, '$');
  output = resolve(output); insist(!existsSync(output), 'output must be a new directory');
  mkdirSync(output);
  try {
    writeFileSync(join(output, 'install.sh'), script, { mode: 0o755 });
    if (windowsScript !== null) writeFileSync(join(output, 'install.ps1'), windowsScript);
    for (let i = 0; i < targets.length; i++) {
      const filename = name(targets[i]);
      writeFileSync(join(output, filename), inputs[i]);
      writeFileSync(join(output, `${filename}.sha256`), `${digests[i]}  ${filename}\n`);
    }
  } catch (error) { rmSync(output, { recursive: true, force: true }); throw error; }
  return { commit, releaseUrl, output, digests: Object.fromEntries(targets.map((p, i) => [p, digests[i]])) };
}
function releaseBase(value: string) {
  const url = new URL(value);
  insist(!url.username && !url.password && !url.search && !url.hash, 'release URL cannot contain credentials, query or fragment');
  const loopback = url.protocol === 'http:' && ['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname);
  insist(loopback || (url.protocol === 'https:' && url.hostname === 'github.com' && /^\/1vecera\/richview\/releases\/download\/[^/]+\/?$/.test(url.pathname)), 'release URL must be a GitHub release download URL or loopback HTTP');
  return { url: url.href.replace(/\/$/, ''), loopback, origin: url.origin };
}
async function download(url: string, base: ReturnType<typeof releaseBase>, fetcher: typeof fetch) {
  for (let redirects = 0; redirects <= 5; redirects++) {
    const response = await fetcher(url, { redirect: 'manual', signal: AbortSignal.timeout(60_000) });
    if (response.status >= 300 && response.status < 400) {
      const location = response.headers.get('location'); insist(location && redirects < 5, 'missing or excessive release redirect');
      const next = new URL(location, url);
      insist(base.loopback ? next.origin === base.origin : next.protocol === 'https:', 'release redirect left its allowed origin or HTTPS');
      url = next.href; continue;
    }
    insist(response.status === 200, `release asset returned HTTP ${response.status}: ${url}`);
    const limit = 100 * 1024 * 1024, declared = response.headers.get('content-length');
    if (declared !== null) insist(/^\d+$/.test(declared) && Number(declared) <= limit, 'release asset exceeds 100 MiB');
    const reader = response.body?.getReader(), chunks: Buffer[] = [];
    let size = 0;
    if (reader) {
      try {
        while (true) {
          const { value, done } = await reader.read();
          if (done) break;
          size += value.length;
          if (size > limit) { await reader.cancel(); throw new Error('release asset exceeds 100 MiB'); }
          chunks.push(Buffer.from(value));
        }
      } finally { reader.releaseLock(); }
    }
    return Buffer.concat(chunks, size);
  }
  throw new Error('too many release redirects');
}
interface VerificationOptions {
  commit: string; installer: string; windowsInstaller?: string; releaseUrl?: string; fetcher?: typeof fetch;
}
export async function verifyPublished({ commit, installer, windowsInstaller, releaseUrl, fetcher = fetch }: VerificationOptions) {
  insist(commitOk(commit), 'approved commit must be a full lowercase Git commit');
  const trusted = regular(installer), values = stamps(trusted.toString('utf8'));
  insist(values.release_commit === commit && /^[0-9a-f]{64}$/.test(values.release_macos_sha256) && /^[0-9a-f]{64}$/.test(values.release_linux_sha256), 'installer stamps differ from the approved release');
  const stampedBase = releaseBase(values.release_base_url);
  insist(!stampedBase.loopback, 'installer release URL must name a GitHub release');
  const base = releaseBase(releaseUrl ?? values.release_base_url);
  insist(base.loopback || base.url === stampedBase.url, 'release URL differs from the installer stamp');
  const expected = [values.release_macos_sha256, values.release_linux_sha256];
  const targets = [...platforms], files: [string, Buffer][] = [['install.sh', trusted]];
  if (windowsInstaller !== undefined) {
    const script = regular(windowsInstaller), preview = stamps(script.toString('utf8'), windowsFields, '$');
    insist(preview.release_commit === commit && preview.release_base_url === values.release_base_url && /^[0-9a-f]{64}$/.test(preview.release_windows_sha256), 'Windows installer stamps differ from the approved release');
    expected.push(preview.release_windows_sha256); targets.push(windowsPlatform); files.push(['install.ps1', script]);
  }
  let notices: Buffer | undefined;
  // The public command uses latest, so it must resolve to these same approved bytes.
  const endpoints = base.loopback ? [base] : [base, { ...base, url: 'https://github.com/1vecera/richview/releases/latest/download' }];
  for (const endpoint of endpoints) {
    for (const [file, local] of files) {
      const live = await download(`${endpoint.url}/${file}`, endpoint, fetcher);
      insist(live.equals(local), `published installer differs from the locally assembled installer: ${file}`);
    }
    for (let i = 0; i < targets.length; i++) {
      const bytes = await download(`${endpoint.url}/${name(targets[i])}`, endpoint, fetcher);
      insist(hash(bytes) === expected[i], `published archive digest mismatch: ${targets[i]}`);
      const actualNotices = identity(bytes, commit, targets[i]);
      if (notices) insist(notices.equals(actualNotices), 'native archives have different third-party notices');
      else notices = actualNotices;
    }
  }
  return { commit, releaseUrl: base.url, digests: Object.fromEntries(targets.map((p, i) => [p, expected[i]])) };
}
function options(args: string[]) {
  const out: Record<string, string | true | undefined> = {};
  while (args.length) {
    const key = args.shift()!;
    if (key === '--verify-published') { insist(!out.verify, 'duplicate --verify-published'); out.verify = true; continue; }
    insist(['--commit', '--tag', '--macos', '--linux', '--windows', '--output', '--installer', '--windows-installer', '--release-url'].includes(key) && args.length && !Object.hasOwn(out, key), `invalid option: ${key}`);
    out[key] = args.shift();
  }
  const needed = out.verify ? ['--commit', '--installer'] : ['--commit', '--tag', '--macos', '--linux', '--output'];
  const optional = out.verify ? ['verify', '--release-url', '--windows-installer'] : ['--windows'];
  insist(needed.every(key => out[key]) && Object.keys(out).every(key => needed.includes(key) || optional.includes(key)), 'usage: node scripts/assemble-release.ts --commit COMMIT --tag TAG --macos ARCHIVE --linux ARCHIVE [--windows ARCHIVE] --output NEW_DIR | --verify-published --commit COMMIT --installer FILE [--windows-installer FILE] [--release-url URL]');
  return out as Record<'--commit' | '--installer' | '--tag' | '--macos' | '--linux' | '--output', string> &
    Partial<Record<'--windows-installer' | '--release-url' | '--windows', string>> & { verify?: true };
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const o = options(process.argv.slice(2));
    const result = o.verify ? await verifyPublished({ commit: o['--commit'], installer: o['--installer'], windowsInstaller: o['--windows-installer'], releaseUrl: o['--release-url'] }) : assemble({ commit: o['--commit'], tag: o['--tag'], macos: o['--macos'], linux: o['--linux'], windows: o['--windows'], output: o['--output'] });
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  } catch (error) { process.stderr.write(`release assembly: ${error instanceof Error ? error.message : String(error)}\n`); process.exitCode = 1; }
}
