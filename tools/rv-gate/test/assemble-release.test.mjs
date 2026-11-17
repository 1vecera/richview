// Host reason (ARC-03): tests release assembly and the published-byte verifier with local archives and HTTP.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { assemble, verifyPublished } from '../../../scripts/assemble-release.mjs';

const tag = 'v1', commit = 'a'.repeat(40), mac = 'aarch64-apple-darwin', linux = 'x86_64-unknown-linux-gnu';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const name = platform => `richview-${platform}.tar.gz`;
function fixture(t) {
  mkdirSync('tmp', { recursive: true });
  const dir = mkdtempSync(resolve('tmp/release-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  let next = 0;
  function archive(platform, revision = commit, notice = 'third-party notice') {
    const suffix = next++;
    const folder = join(dir, `payload-${platform}-${suffix}`); mkdirSync(folder);
    writeFileSync(join(folder, 'BUILD'), `commit=${revision}\nplatform=${platform}\n`);
    const files = ['./BUILD'];
    if (notice !== null) {
      mkdirSync(join(folder, 'notices'));
      writeFileSync(join(folder, 'notices/THIRD-PARTY-NOTICES.txt'), notice);
      files.push('./notices/THIRD-PARTY-NOTICES.txt');
    }
    const output = join(dir, `${platform}-${suffix}.tar.gz`);
    execFileSync('tar', ['-czf', output, '-C', folder, ...files]); return output;
  }
  return { dir, macos: archive(mac), linux: archive(linux), archive };
}
function run(args) {
  return new Promise((accept, reject) => {
    const child = spawn(process.execPath, ['scripts/assemble-release.mjs', ...args], { stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '', stderr = ''; child.stdout.on('data', b => { stdout += b; }); child.stderr.on('data', b => { stderr += b; });
    child.on('error', reject); child.on('close', status => accept({ status, stdout, stderr }));
  });
}
test('assembly binds both archive BUILD records, stamps digests and refuses a mismatched build', async t => {
  const f = fixture(t), output = join(f.dir, 'release');
  const result = await run(['--commit', commit, '--tag', tag, '--macos', f.macos, '--linux', f.linux, '--output', output]);
  assert.equal(result.status, 0, result.stderr);
  const record = JSON.parse(result.stdout), script = readFileSync(join(output, 'install.sh'), 'utf8');
  assert.equal(record.commit, commit); assert.ok(script.includes(`release_commit='${commit}'`));
  assert.equal(record.releaseUrl, `https://github.com/1vecera/richview/releases/download/${tag}`);
  assert.ok(script.includes(`release_base_url='${record.releaseUrl}'`));
  for (const [platform, input, key] of [[mac, f.macos, 'release_macos_sha256'], [linux, f.linux, 'release_linux_sha256']]) {
    const digest = sha(readFileSync(input)); assert.equal(record.digests[platform], digest);
    assert.ok(script.includes(`${key}='${digest}'`));
    assert.deepEqual(readFileSync(join(output, name(platform))), readFileSync(input));
    assert.equal(readFileSync(join(output, `${name(platform)}.sha256`), 'utf8'), `${digest}  ${name(platform)}\n`);
  }
  assert.throws(() => assemble({ commit, tag, macos: f.macos, linux: f.archive(linux, 'b'.repeat(40)), output: join(f.dir, 'wrong') }), /BUILD differs/);
  assert.throws(() => assemble({ commit, tag, macos: f.macos, linux: f.archive(linux, commit, null), output: join(f.dir, 'missing') }), /lacks third-party notices/);
  assert.throws(() => assemble({ commit, tag, macos: f.macos, linux: f.archive(linux, commit, 'different notice'), output: join(f.dir, 'different') }), /different third-party notices/);
  assert.equal(existsSync(join(f.dir, 'wrong')), false);
  assert.throws(() => assemble({ commit, tag, macos: f.macos, linux: f.linux, output }), /new directory/);
  for (const invalid of ['', '../bad', 'v1?bad', 'v1.lock']) assert.throws(() => assemble({ commit, tag: invalid, macos: f.macos, linux: f.linux, output: join(f.dir, 'invalid') }), /tag/);
  const special = assemble({ commit, tag: "release/it's-v1", macos: f.macos, linux: f.linux, output: join(f.dir, 'special') });
  assert.ok(special.releaseUrl.endsWith('release%2Fit%27s-v1'));
  assert.ok(readFileSync(join(special.output, 'install.sh'), 'utf8').includes(`release_base_url='${special.releaseUrl}'`));
});

test('published verification checks installer, both archives, commit and redirect authority', async t => {
  const f = fixture(t), output = join(f.dir, 'release'); assemble({ commit, tag, macos: f.macos, linux: f.linux, output });
  const paths = ['install.sh', name(mac), name(linux)];
  const bytes = Object.fromEntries(paths.map(path => [path, readFileSync(join(output, path))]));
  const state = { missing: '', changed: '', crossHost: false, requests: [] };
  const server = createServer((request, response) => {
    state.requests.push(request.url);
    if (request.url.startsWith('/release/')) {
      if (state.crossHost) { response.writeHead(302, { location: `http://localhost:${server.address().port}/escaped` }); response.end(); return; }
      response.writeHead(302, { location: request.url.replace('/release/', '/assets/') }); response.end(); return;
    }
    const file = request.url.slice('/assets/'.length);
    if (file === state.missing || !bytes[file]) { response.writeHead(404); response.end(); return; }
    response.end(file === state.changed ? Buffer.from('tampered') : bytes[file]);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  const releaseUrl = `http://127.0.0.1:${server.address().port}/release`, installer = join(output, 'install.sh');
  let result = await run(['--verify-published', '--commit', commit, '--installer', installer, '--release-url', releaseUrl]);
  assert.equal(result.status, 0, result.stderr); assert.equal(JSON.parse(result.stdout).commit, commit);
  for (const changed of paths) {
    state.changed = changed; await assert.rejects(verifyPublished({ commit, installer, releaseUrl }), /differs|digest mismatch/);
  }
  state.changed = ''; state.missing = name(linux);
  await assert.rejects(verifyPublished({ commit, installer, releaseUrl }), /HTTP 404/);
  state.missing = ''; await assert.rejects(verifyPublished({ commit: 'b'.repeat(40), installer, releaseUrl }), /stamps differ/);
  await assert.rejects(verifyPublished({ commit, installer, releaseUrl: 'https://github.com/1vecera/richview/releases/download/v2', fetcher: async () => { throw new Error('unexpected fetch'); } }), /differs from the installer stamp/);
  state.crossHost = true; state.requests = [];
  await assert.rejects(verifyPublished({ commit, installer, releaseUrl }), /allowed origin/);
  assert.deepEqual(state.requests, ['/release/install.sh']);
});

test('the public latest URL must serve the same approved release', async t => {
  const f = fixture(t), output = join(f.dir, 'release'); assemble({ commit, tag, macos: f.macos, linux: f.linux, output });
  const installer = join(output, 'install.sh'), requested = [];
  const fetcher = async url => {
    requested.push(url);
    if (url.includes('/latest/')) return new Response('', { status: 404 });
    const file = url.split('/').at(-1); return new Response(readFileSync(join(output, file)));
  };
  await assert.rejects(verifyPublished({ commit, installer, fetcher }), /HTTP 404/);
  assert.ok(requested[0].startsWith('https://github.com/1vecera/richview/releases/download/v1/'));
  assert.ok(requested.some(url => url.includes('/latest/')));
});

test('the published verifier refuses an oversized asset before reading it', async t => {
  const f = fixture(t), output = join(f.dir, 'release'); assemble({ commit, tag, macos: f.macos, linux: f.linux, output });
  const installer = join(output, 'install.sh');
  const fetcher = async () => new Response('x', { headers: { 'content-length': String(101 * 1024 * 1024) } });
  await assert.rejects(verifyPublished({ commit, installer, releaseUrl: 'https://github.com/1vecera/richview/releases/download/v1', fetcher }), /exceeds 100 MiB/);
});

test('an optional Windows archive gets its own stamped installer and is verified when published', async t => {
  const f = fixture(t), windows = 'x86_64-pc-windows-msvc', preview = f.archive(windows), output = join(f.dir, 'release'), plain = join(f.dir, 'plain');
  const result = await run(['--commit', commit, '--tag', tag, '--macos', f.macos, '--linux', f.linux, '--windows', preview, '--output', output]);
  assert.equal(result.status, 0, result.stderr);
  const record = JSON.parse(result.stdout), digest = sha(readFileSync(preview)), script = readFileSync(join(output, 'install.ps1'), 'utf8');
  assert.equal(record.digests[windows], digest);
  for (const stamp of [`$release_commit='${commit}'`, `$release_base_url='${record.releaseUrl}'`, `$release_windows_sha256='${digest}'`]) assert.ok(script.split('\n').includes(stamp), stamp);
  assert.ok(!script.includes('UNSTAMPED'));
  assert.deepEqual(readFileSync(join(output, name(windows))), readFileSync(preview));
  assert.equal(readFileSync(join(output, `${name(windows)}.sha256`), 'utf8'), `${digest}  ${name(windows)}\n`);
  // The two-platform release is the same with and without the preview.
  assemble({ commit, tag, macos: f.macos, linux: f.linux, output: plain });
  assert.deepEqual(readdirSync(plain).sort(), ['install.sh', name(mac), `${name(mac)}.sha256`, name(linux), `${name(linux)}.sha256`].sort());
  for (const file of readdirSync(plain)) assert.deepEqual(readFileSync(join(output, file)), readFileSync(join(plain, file)), file);
  for (const [archive, message] of [[f.archive(windows, 'b'.repeat(40)), /BUILD differs/], [f.macos, /BUILD differs/], [f.archive(windows, commit, 'different notice'), /different third-party notices/]]) {
    assert.throws(() => assemble({ commit, tag, macos: f.macos, linux: f.linux, windows: archive, output: join(f.dir, 'refused') }), message);
    assert.equal(existsSync(join(f.dir, 'refused')), false);
  }
  const installer = join(output, 'install.sh'), windowsInstaller = join(output, 'install.ps1'), state = { changed: '', requested: [] };
  const fetcher = async url => { const file = url.split('/').at(-1); state.requested.push(file); return new Response(file === state.changed ? 'tampered' : readFileSync(join(output, file))); };
  const verified = await verifyPublished({ commit, installer, windowsInstaller, fetcher });
  assert.equal(verified.digests[windows], digest);
  assert.equal(state.requested.filter(file => file === 'install.ps1').length, 2, 'the tag and latest both serve the Windows installer');
  assert.equal(state.requested.filter(file => file === name(windows)).length, 2);
  for (const changed of ['install.ps1', name(windows)]) {
    state.changed = changed; await assert.rejects(verifyPublished({ commit, installer, windowsInstaller, fetcher }), /differs|digest mismatch/);
  }
  // Without the Windows installer the release is verified as two platforms and asks for nothing more.
  state.requested = []; await verifyPublished({ commit, installer, fetcher });
  assert.ok(!state.requested.includes('install.ps1') && !state.requested.includes(name(windows)));
  const other = assemble({ commit, tag: 'v2', macos: f.macos, linux: f.linux, windows: preview, output: join(f.dir, 'other') });
  await assert.rejects(verifyPublished({ commit, installer, windowsInstaller: join(other.output, 'install.ps1'), fetcher: async () => { throw new Error('unexpected fetch'); } }), /Windows installer stamps differ/);
});
