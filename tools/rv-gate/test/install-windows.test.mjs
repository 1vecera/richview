// Host reason (ARC-03): executes the PowerShell installer against owned loopback archives and temporary prefixes.
// The gate host has no Windows, so this runs the script in PowerShell 7 with the three environment
// values Windows sets (OS, PROCESSOR_ARCHITECTURE, SystemRoot) and the host's tar as System32's.
// It skips where pwsh is not installed, and it does not start the Windows executables.
import assert from 'node:assert/strict';
import { execFileSync, spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import { join, resolve } from 'node:path';
import test from 'node:test';

const installer = resolve('scripts/install.ps1');
const commit = '1'.repeat(40), archiveName = 'richview-x86_64-pc-windows-msvc.tar.gz';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
// PowerShell on Unix keeps its own caches in HOME; they are not the installer's.
const left = home => readdirSync(home).filter(name => !['.cache', '.dotnet', '.local'].includes(name));
const skip = spawnSync('pwsh', ['-NoLogo', '-NoProfile', '-Command', 'exit 0']).status === 0 ? false : 'PowerShell 7 (pwsh) is not installed';

function run(args, env, input) {
  return new Promise((accept, reject) => {
    const child = spawn('pwsh', ['-NoLogo', '-NoProfile', '-NonInteractive', ...args], { env, timeout: 60000, stdio: [input === undefined ? 'ignore' : 'pipe', 'pipe', 'pipe'] });
    let stdout = '', stderr = ''; child.stdout.on('data', b => { stdout += b; }); child.stderr.on('data', b => { stderr += b; });
    child.once('error', reject); // PowerShell colours an error and wraps it to the terminal width; the tests read one line.
    child.once('close', status => accept({ status, stdout, stderr: stderr.replace(/\x1B\[[0-9;]*m/g, '').replace(/\s*\n\s*\|\s*/g, ' ') }));
    if (input !== undefined) child.stdin.end(input);
  });
}

async function fixture(fn, { mutate = () => {}, digest, members = ['.'] } = {}) {
  mkdirSync('tmp', { recursive: true });
  const dir = mkdtempSync(resolve('tmp/install-windows-test-')), home = join(dir, "home's spaces"), payload = join(dir, 'payload'), system = join(dir, 'windows/System32');
  mkdirSync(home); mkdirSync(join(payload, 'bin'), { recursive: true }); mkdirSync(join(payload, 'notices')); mkdirSync(system, { recursive: true });
  symlinkSync(execFileSync('/bin/sh', ['-c', 'command -v tar'], { encoding: 'utf8' }).trim(), join(system, 'tar.exe'));
  const files = {
    'bin/richview.exe': 'MZ fixture CLI', 'bin/richview-sql-worker.exe': 'MZ fixture worker',
    'uninstall.ps1': readFileSync(installer), 'notices/LICENSE': 'fixture license\n',
    BUILD: `commit=${commit}\nplatform=x86_64-pc-windows-msvc\n`,
  };
  for (const [name, bytes] of Object.entries(files)) writeFileSync(join(payload, name), bytes);
  writeFileSync(join(payload, 'SHA256SUMS'), Object.entries(files).map(([name, bytes]) => `${sha(bytes)}  ${name}\n`).join(''));
  mutate(payload, dir);
  const archive = join(dir, archiveName);
  execFileSync('tar', ['-czf', archive, '--format=ustar', '-C', payload, ...members], { env: { ...process.env, COPYFILE_DISABLE: '1' } });
  const bytes = readFileSync(archive), requests = [], script = join(dir, 'install.ps1');
  const stamp = { release_commit: commit, release_base_url: 'https://github.com/1vecera/richview/releases/download/v1', release_windows_sha256: digest ?? sha(bytes) };
  writeFileSync(script, Object.entries(stamp).reduce((text, [field, value]) => text.replace(`$${field}='UNSTAMPED'`, () => `$${field}='${value}'`), readFileSync(installer, 'utf8')));
  const server = http.createServer((request, response) => {
    requests.push(request.url);
    if (request.url === `/${archiveName}`) response.end(bytes); else { response.statusCode = 404; response.end(); }
  });
  await new Promise(accept => server.listen(0, '127.0.0.1', accept));
  const prefix = join(home, '.richview'), base = `http://127.0.0.1:${server.address().port}`;
  const windows = { PATH: process.env.PATH, HOME: home, TMPDIR: home, OS: 'Windows_NT', PROCESSOR_ARCHITECTURE: 'AMD64', SystemRoot: join(dir, 'windows') };
  const install = (args = [], env = {}) => run(['-File', script, ...(args.includes('-BaseUrl') ? [] : ['-BaseUrl', base]), ...args], { ...windows, ...env });
  const uninstall = () => run(['-File', join(prefix, 'uninstall.ps1'), '-Uninstall'], windows);
  try { await fn({ dir, home, prefix, base, script, windows, install, uninstall, requests }); }
  finally { await new Promise(accept => server.close(accept)); rmSync(dir, { recursive: true, force: true }); }
}

test('install puts both executables in place and self-uninstall leaves the temporary HOME empty', { skip }, async () => {
  await fixture(async ({ home, prefix, install, uninstall, requests }) => {
    const installed = await install(['-Commit', commit]); assert.equal(installed.status, 0, JSON.stringify(installed));
    assert.match(installed.stdout, /Agent MCP command: & '.*richview\.exe' mcp/); assert.match(installed.stdout, /Run your own report server, RichView Cowork: & '.*richview\.exe' server --help/); assert.match(installed.stdout, /choose RichView Cowork hosted by RichView at app\.richview\.ai, or your own server; no hosted service was started/); assert.doesNotMatch(installed.stdout, /not available on Windows/);
    assert.ok(installed.stdout.includes(`-File '${join(prefix, 'uninstall.ps1').replaceAll("'", "''")}' -Uninstall`));
    assert.equal(readFileSync(join(prefix, 'bin/richview.exe'), 'utf8'), 'MZ fixture CLI');
    assert.ok(existsSync(join(prefix, 'bin/richview-sql-worker.exe')));
    assert.deepEqual(readFileSync(join(prefix, '.richview-files'), 'utf8').split('\n'), ['./BUILD', './SHA256SUMS', './bin', './bin/richview-sql-worker.exe', './bin/richview.exe', './notices', './notices/LICENSE', './uninstall.ps1', '']);
    assert.deepEqual(left(home), ['.richview']);
    const removed = await uninstall(); assert.equal(removed.status, 0, JSON.stringify(removed));
    assert.match(removed.stdout, /SQL snapshots and upload journals/); assert.match(removed.stdout, /Remove any PATH entry/);
    assert.deepEqual(left(home), []);
    assert.deepEqual(requests, [`/${archiveName}`]);
  });
});

test('the piped form installs with options and leaves the session as it was', { skip }, async () => {
  await fixture(async ({ home, prefix, base, script, windows }) => {
    const piped = await run(['-Command', `& ([scriptblock]::Create((Get-Content -Raw -LiteralPath $env:RV_TEST_SCRIPT))) -BaseUrl ${base}; "session: $ErrorActionPreference $(Test-Path variable:release_commit) $(Test-Path function:Fail)"`], { ...windows, RV_TEST_SCRIPT: script });
    assert.equal(piped.status, 0, JSON.stringify(piped)); assert.match(piped.stdout, /Installed RichView build 1{40}/); assert.match(piped.stdout, /session: Continue False False/);
    assert.ok(existsSync(join(prefix, 'bin/richview.exe')));
    // `irm | iex` of the source file: it refuses, and the caller's window stays open to print the next line.
    const unstamped = await run(['-Command', 'try { Get-Content -Raw -LiteralPath $env:RV_TEST_SCRIPT | Invoke-Expression } catch { "refused: $_" }; "still here: $ErrorActionPreference"'], { ...windows, RV_TEST_SCRIPT: installer, HOME: join(home, '..') });
    assert.equal(unstamped.status, 0, JSON.stringify(unstamped));
    assert.match(unstamped.stdout, /refused: richview install: installer has not been assembled for a release/); assert.match(unstamped.stdout, /still here: Continue/);
  });
});

test('a wrong checksum, a wrong commit and another platform refuse before anything is installed', { skip }, async () => {
  await fixture(async ({ home, install, requests }) => {
    const wrong = await install(); assert.notEqual(wrong.status, 0); assert.match(wrong.stderr, /archive checksum mismatch; nothing installed/);
    assert.deepEqual(left(home), []);
    requests.length = 0;
    for (const [args, env, message] of [
      [['-Commit', '2'.repeat(40)], {}, /requested commit differs/],
      [[], { OS: '' }, /installs the Windows build/],
      [[], { PROCESSOR_ARCHITECTURE: 'ARM64' }, /On Windows RichView supports x64/],
      [[], { PROCESSOR_ARCHITECTURE: 'x86', PROCESSOR_ARCHITEW6432: 'ARM64' }, /On Windows RichView supports x64/],
      [['-BaseUrl', 'http://example.com/release'], {}, /requires HTTPS/],
      [['-BaseUrl', 'https://user@example.com/release'], {}, /without credentials/],
      [['-Prefix', 'relative'], {}, /prefix must be absolute/],
      [['-Prefix', join(home, 'missing/richview')], {}, /parent folder must exist/],
    ]) {
      const result = await install(args, env); assert.notEqual(result.status, 0, JSON.stringify(args)); assert.match(result.stderr, message);
    }
    assert.deepEqual(requests, []); assert.deepEqual(left(home), []);
  }, { digest: '0'.repeat(64) });
});

test('an altered payload, a traversal path and a link are refused with a valid outer checksum', { skip }, async () => {
  const cases = [
    [{ mutate: payload => writeFileSync(join(payload, 'notices/LICENSE'), 'changed') }, /file checksum mismatch: notices\/LICENSE/],
    [{ mutate: payload => writeFileSync(join(payload, 'bin/extra.exe'), 'MZ') }, /unlisted or duplicate archive files/],
    [{ mutate: payload => writeFileSync(join(payload, 'BUILD'), `commit=${'2'.repeat(40)}\nplatform=x86_64-pc-windows-msvc\n`) }, /file checksum mismatch: BUILD/],
    [{ mutate: payload => rmSync(join(payload, 'bin/richview-sql-worker.exe')) }, /missing regular file: bin\/richview-sql-worker.exe/],
    [{ mutate: payload => writeFileSync(join(payload, '.richview-install'), 'owned') }, /installer-owned metadata/],
    [{ mutate: (payload, dir) => writeFileSync(join(dir, 'escape'), 'outside'), members: ['.', '../escape'] }, /unsafe archive path/],
    [{ mutate: payload => symlinkSync('BUILD', join(payload, 'link')) }, /link or special file/],
  ];
  for (const [options, message] of cases) {
    await fixture(async ({ home, install }) => {
      const result = await install(); assert.notEqual(result.status, 0, String(message)); assert.match(result.stderr, message);
      assert.deepEqual(left(home), []);
    }, options);
  }
});

test('uninstall keeps changed files and added content, and an existing prefix is never overwritten', { skip }, async () => {
  await fixture(async ({ home, prefix, install, uninstall }) => {
    assert.equal((await install()).status, 0);
    const again = await install(); assert.notEqual(again.status, 0); assert.match(again.stderr, /prefix already exists/);
    writeFileSync(join(prefix, 'my-report.rmd'), 'keep this');
    let result = await uninstall(); assert.notEqual(result.status, 0); assert.match(result.stderr, /additional or missing content/); assert.match(result.stderr, /added: \.\/my-report\.rmd/);
    assert.equal(readFileSync(join(prefix, 'my-report.rmd'), 'utf8'), 'keep this'); rmSync(join(prefix, 'my-report.rmd'));
    const cli = readFileSync(join(prefix, 'bin/richview.exe')); rmSync(join(prefix, 'bin/richview.exe'));
    result = await uninstall(); assert.notEqual(result.status, 0); assert.match(result.stderr, /missing: \.\/bin\/richview\.exe/);
    writeFileSync(join(prefix, 'bin/richview.exe'), cli); writeFileSync(join(prefix, 'notices/LICENSE'), 'local edit');
    result = await uninstall(); assert.notEqual(result.status, 0); assert.match(result.stderr, /file checksum mismatch: notices\/LICENSE/); assert.match(result.stderr, /restore this file before retrying/);
    assert.equal(readFileSync(join(prefix, 'notices/LICENSE'), 'utf8'), 'local edit');
    writeFileSync(join(prefix, '.richview-install'), 'richview-install-v1 forged forged\n');
    result = await uninstall(); assert.notEqual(result.status, 0); assert.match(result.stderr, /ownership record changed/);
    assert.deepEqual(left(home), ['.richview']);
    mkdirSync(join(home, 'plain')); writeFileSync(join(home, 'plain/file'), 'not ours');
    result = await run(['-File', join(prefix, 'uninstall.ps1'), '-Uninstall', '-Prefix', join(home, 'plain')], { PATH: process.env.PATH, HOME: home, OS: 'Windows_NT', PROCESSOR_ARCHITECTURE: 'AMD64' });
    assert.notEqual(result.status, 0); assert.match(result.stderr, /not an owned installation/); assert.equal(readFileSync(join(home, 'plain/file'), 'utf8'), 'not ours');
  });
});
