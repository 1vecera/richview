// Host reason (ARC-03): executes the shell installer against owned loopback archives and temporary prefixes.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import { isAbsolute, join, resolve } from 'node:path';
import { arch, platform } from 'node:os';
import { gzipSync, gunzipSync } from 'node:zlib';
import test from 'node:test';

const installer = resolve('scripts/install.sh');
const commit = '1'.repeat(40);
const target = `${arch() === 'arm64' ? 'aarch64' : arch() === 'x64' ? 'x86_64' : arch()}-${platform() === 'darwin' ? 'apple-darwin' : 'unknown-linux-gnu'}`;
const archiveName = `richview-${target}.tar.gz`;
const sha = b => createHash('sha256').update(b).digest('hex');

function run(command, args, home, overrides = {}, { input, cwd = home } = {}) {
  const started = performance.now(), timeoutMs = 10000;
  let executable = null;
  if (isAbsolute(command)) {
    try { const bytes = readFileSync(command), stat = statSync(command); executable = { size: stat.size, mode: stat.mode & 0o777, sha256: sha(bytes), shebang: bytes.toString('utf8').split('\n', 1)[0] }; }
    catch (error) { executable = { error: error.code }; }
  }
  return new Promise((accept, reject) => {
    const child = spawn(command, args, { cwd, env: { PATH: process.env.PATH, HOME: home, TMPDIR: home, LC_ALL: 'C', ...overrides }, timeout: timeoutMs, stdio: [input === undefined ? 'ignore' : 'pipe', 'pipe', 'pipe'] });
    const spawnReturnedMs = performance.now() - started;
    let stdout = '', stderr = '', spawnedMs = null, exitedMs = null, firstStdoutMs = null, firstStderrMs = null;
    const diagnostic = (status, signal) => ({ status, signal, stdout, stderr, command, args, cwd, pid: child.pid, parentPid: process.pid, timeoutMs,
      elapsedMs: performance.now() - started, spawnReturnedMs, spawnedMs, exitedMs, firstStdoutMs, firstStderrMs, executable });
    child.once('spawn', () => { spawnedMs = performance.now() - started; });
    child.once('exit', () => { exitedMs = performance.now() - started; });
    child.stdout.on('data', b => { firstStdoutMs ??= performance.now() - started; stdout += b; });
    child.stderr.on('data', b => { firstStderrMs ??= performance.now() - started; stderr += b; });
    child.once('error', error => reject(Object.assign(error, { childDiagnostic: diagnostic(null, null) })));
    child.once('close', (status, signal) => accept(diagnostic(status, signal)));
    if (input !== undefined) { child.stdin.on('error', error => { if (error.code !== 'EPIPE') reject(error); }); child.stdin.end(input); }
  });
}

async function fixture(fn, mutate = () => {}, wrongSha = false, options = {}) {
  mkdirSync('tmp', { recursive: true });
  const dir = mkdtempSync(resolve('tmp/install-test-')), home = join(dir, "home's spaces"), payload = join(dir, 'payload'), cwd = join(dir, 'working');
  mkdirSync(home); mkdirSync(cwd); mkdirSync(join(payload, 'bin'), { recursive: true }); mkdirSync(join(payload, 'notices'));
  const files = {
    'bin/richview': options.cli ?? `#!/bin/sh\nif [ "$1" = --version ]; then printf 'richview 3.0.0 (commit ${commit.slice(0, 10)}, runtime 000000000000)\\n'; else printf "fixture CLI\\n"; fi\n`,
    'bin/richview-sql-worker': '#!/bin/sh\nexit 0\n',
    'uninstall.sh': readFileSync(installer), 'notices/LICENSE': 'fixture license\n',
    BUILD: `commit=${commit}\nplatform=${target}\n`,
  };
  for (const [name, bytes] of Object.entries(files)) { writeFileSync(join(payload, name), bytes); if (name.startsWith('bin/') || name.endsWith('.sh')) chmodSync(join(payload, name), 0o755); }
  writeFileSync(join(payload, 'SHA256SUMS'), Object.entries(files).map(([name, bytes]) => `${sha(bytes)}  ${name}\n`).join(''));
  mutate(payload);
  const archive = join(dir, archiveName);
  execFileSync('tar', ['-czf', archive, '--format=ustar', '-C', payload, '.'], { env: { ...process.env, COPYFILE_DISABLE: '1' } });
  let bytes = readFileSync(archive), requests = [];
  const scriptPath = join(dir, 'install.sh');
  const stamp = digest => {
    const text = readFileSync(installer, 'utf8').replace("release_commit='UNSTAMPED'", `release_commit='${commit}'`).replace("release_base_url='UNSTAMPED'", "release_base_url='https://github.com/1vecera/richview/releases/download/v1'").replace("release_macos_sha256='UNSTAMPED'", `release_macos_sha256='${digest}'`).replace("release_linux_sha256='UNSTAMPED'", `release_linux_sha256='${digest}'`);
    writeFileSync(scriptPath, text);
    return Buffer.from(text);
  };
  let script = stamp(wrongSha ? '0'.repeat(64) : sha(bytes));
  const server = http.createServer((request, response) => {
    requests.push(request.url);
    if (options.respond?.(request, response, bytes)) return;
    if (request.url === `/${archiveName}.sha256`) response.end(`${sha(bytes)}  ${archiveName}\n`);
    else if ([`/${archiveName}`, `/assets/${archiveName}`].includes(request.url)) response.end(bytes);
    else { response.statusCode = 404; response.end(); }
  });
  await new Promise(accept => server.listen(0, '127.0.0.1', accept));
  const prefix = join(home, '.richview'), base = `http://127.0.0.1:${server.address().port}`;
  const install = (args = [], overrides = {}) => run('sh', [scriptPath, '--base-url', base, '--commit', commit, ...args], home, overrides);
  const uninstall = () => run('sh', [join(prefix, 'uninstall.sh'), '--uninstall'], home);
  const pipe = (input, shell = '/bin/sh', overrides = {}) => run(shell, ['-s', '--', '--base-url', base, '--commit', commit], home, overrides, { input, cwd });
  try { await fn({ dir, home, cwd, prefix, install, uninstall, pipe, requests, script, scriptPath, archive, setArchive: value => { bytes = value; script = stamp(sha(value)); }, setUntrustedArchive: value => { bytes = value; }, bytes }); }
  finally { await new Promise(accept => server.close(accept)); rmSync(dir, { recursive: true, force: true }); }
}

test('install, executable launch and self-uninstall leave the temporary HOME empty', async () => {
  await fixture(async ({ home, prefix, install, uninstall, requests }) => {
    const installed = await install(); assert.equal(installed.status, 0, JSON.stringify(installed));
    assert.match(installed.stdout, /MCP command:/); assert.match(installed.stdout, /choose RichView Cowork hosted by RichView at app\.richview\.ai, or your own server/); assert.match(installed.stdout, /no hosted service was started/);
    assert.match(installed.stdout, /Run your own report server, RichView Cowork: '.*' server --help/);
    // The installer names the version the installed CLI reports, with the commit after it.
    const version = await run(join(prefix, 'bin/richview'), ['--version'], home);
    assert.equal(version.status, 0, JSON.stringify(version));
    assert.equal(version.stdout, `richview 3.0.0 (commit ${commit.slice(0, 10)}, runtime 000000000000)\n`, JSON.stringify(version));
    assert.equal(installed.stdout.split('\n')[0], `Installed RichView 3.0.0 (commit ${commit.slice(0, 10)}) in ${prefix}`);
    assert.equal(installed.stderr, '', JSON.stringify(installed));
    const mcpCommand = installed.stdout.split('\n').find(line => line.startsWith('Agent MCP command: ')).slice('Agent MCP command: '.length);
    const mcp = await run('sh', ['-c', mcpCommand], home);
    assert.equal(mcp.status, 0, JSON.stringify(mcp));
    assert.equal(mcp.stdout, 'fixture CLI\n', JSON.stringify(mcp));
    assert.ok(existsSync(join(prefix, 'bin/richview-sql-worker')));
    assert.equal((await uninstall()).status, 0);
    assert.deepEqual(readdirSync(home), []);
    assert.deepEqual(requests, [`/${archiveName}`]);
  });
});

test('a CLI that reports no version still installs, names the build and says why', async () => {
  await fixture(async ({ install }) => {
    const installed = await install(); assert.equal(installed.status, 0, JSON.stringify(installed));
    assert.match(installed.stdout, new RegExp(`^Installed RichView build ${commit} in `));
    assert.match(installed.stderr, /the installed CLI did not report its version; run '.*' --version to see why/);
  }, () => {}, false, { cli: '#!/bin/sh\nexit 3\n' });
});

test('a saved installer downloads from its stamped release after latest changes', async () => {
  await fixture(async ({ dir, home, scriptPath, archive, requests }) => {
    const commands = join(dir, 'commands'), log = join(dir, 'curl-url'); mkdirSync(commands);
    const curl = '#!/bin/sh\noutput=\nlast=\nwhile [ "$#" -gt 0 ]; do\n  if [ "$1" = --output ]; then output=$2; shift 2; continue; fi\n  last=$1; shift\ndone\nprintf "%s\\n" "$last" > "$RV_TEST_CURL_LOG"\n/bin/cp "$RV_TEST_ARCHIVE" "$output"\nprintf 200\n';
    writeFileSync(join(commands, 'curl'), curl); chmodSync(join(commands, 'curl'), 0o755);
    const result = await run('sh', [scriptPath], home, { PATH: `${commands}:${process.env.PATH}`, RV_TEST_CURL_LOG: log, RV_TEST_ARCHIVE: archive });
    assert.equal(result.status, 0, JSON.stringify(result));
    assert.equal(readFileSync(log, 'utf8').trim(), `https://github.com/1vecera/richview/releases/download/v1/${archiveName}`);
    assert.deepEqual(requests, []);
  });
});

test('a truncated stdin script cannot extract into the current directory or reserve a partial prefix', async () => {
  const shells = ['/bin/sh', ...(existsSync('/bin/dash') ? ['/bin/dash'] : [])];
  for (const shell of shells) {
    for (const marker of ['-C "$work/payload"', 'installing=true']) await fixture(async ({ home, cwd, pipe, script }) => {
      const cut = script.indexOf(marker); assert.ok(cut > 0);
      const result = await pipe(script.subarray(0, cut), shell);
      assert.equal(result.signal, null, JSON.stringify(result));
      assert.deepEqual(readdirSync(cwd), []); assert.deepEqual(readdirSync(home), []);
    });
    await fixture(async ({ home, cwd, prefix, pipe, uninstall, script }) => {
      const result = await pipe(script, shell); assert.equal(result.status, 0, JSON.stringify(result));
      assert.ok(existsSync(join(prefix, 'bin/richview'))); assert.deepEqual(readdirSync(cwd), []);
      assert.equal((await uninstall()).status, 0); assert.deepEqual(readdirSync(home), []);
    });
  }
});

test('a wrong download checksum refuses before creating the installation', async () => {
  await fixture(async ({ home, install }) => {
    const result = await install(); assert.notEqual(result.status, 0); assert.match(result.stderr, /archive checksum mismatch/);
    assert.deepEqual(readdirSync(home), []);
  }, () => {}, true);
});

test('a replacement archive and matching sibling checksum cannot override the embedded digest', async () => {
  await fixture(async ({ dir, home, install, requests, setUntrustedArchive }) => {
    const payload = join(dir, 'replacement'); mkdirSync(join(payload, 'bin'), { recursive: true });
    const files = { 'bin/richview': '#!/bin/sh\necho hostile\n', 'bin/richview-sql-worker': '#!/bin/sh\nexit 0\n', 'uninstall.sh': readFileSync(installer), BUILD: `commit=${commit}\nplatform=${target}\n` };
    for (const [file, bytes] of Object.entries(files)) writeFileSync(join(payload, file), bytes);
    writeFileSync(join(payload, 'SHA256SUMS'), Object.entries(files).map(([file, bytes]) => `${sha(bytes)}  ${file}\n`).join(''));
    const archive = join(dir, 'replacement.tar.gz'); execFileSync('tar', ['-czf', archive, '-C', payload, '.']);
    setUntrustedArchive(readFileSync(archive));
    const result = await install(); assert.notEqual(result.status, 0); assert.match(result.stderr, /archive checksum mismatch/);
    assert.deepEqual(readdirSync(home), []); assert.deepEqual(requests, [`/${archiveName}`]);
  });
});

test('unstamped source installer refuses to install', async () => {
  await fixture(async ({ home, requests }) => {
    const result = await run('sh', [installer, '--base-url', 'http://127.0.0.1:9'], home);
    assert.notEqual(result.status, 0); assert.match(result.stderr, /not been assembled/);
    assert.deepEqual(requests, []); assert.deepEqual(readdirSync(home), []);
  });
});

test('a failed copy removes its reserved partial installation and preserves unrelated data', async () => {
  await fixture(async ({ dir, home, install }) => {
    const commands = join(dir, 'commands'); mkdirSync(commands);
    writeFileSync(join(commands, 'cp'), '#!/bin/sh\n/bin/cp "$@"\nexit 37\n'); chmodSync(join(commands, 'cp'), 0o755);
    writeFileSync(join(home, 'notes.txt'), 'keep this');
    const result = await install([], { PATH: `${commands}:${process.env.PATH}` });
    assert.equal(result.status, 37, JSON.stringify(result));
    assert.deepEqual(readdirSync(home), ['notes.txt']);
    assert.equal(readFileSync(join(home, 'notes.txt'), 'utf8'), 'keep this');
  });
});

test('an altered payload with a valid outer checksum is refused', async () => {
  await fixture(async ({ home, install }) => {
    const result = await install(); assert.notEqual(result.status, 0); assert.match(result.stderr, /file checksum mismatch/);
    assert.deepEqual(readdirSync(home), []);
  }, payload => writeFileSync(join(payload, 'bin/richview'), '#!/bin/sh\nexit 1\n'));
});

test('archive links and traversal paths cannot write outside the owned staging directory', async () => {
  await fixture(async ({ home, install }) => {
    const result = await install(); assert.notEqual(result.status, 0); assert.match(result.stderr, /link or special file/);
    assert.deepEqual(readdirSync(home), []);
  }, payload => symlinkSync('/outside', join(payload, 'link')));
  await fixture(async ({ dir, home, bytes, setArchive, install }) => {
    const tar = gunzipSync(bytes); let offset = 0;
    while (offset < tar.length && tar[offset]) {
      if (tar[offset + 156] === 48) {
        tar.fill(0, offset, offset + 100); tar.write('../escaped', offset);
        tar.fill(32, offset + 148, offset + 156);
        const checksum = [...tar.subarray(offset, offset + 512)].reduce((a, b) => a + b, 0).toString(8).padStart(6, '0');
        tar.write(`${checksum}\0 `, offset + 148); break;
      }
      const size = parseInt(tar.subarray(offset + 124, offset + 136).toString().replace(/\0/g, '').trim(), 8) || 0;
      offset += 512 + Math.ceil(size / 512) * 512;
    }
    assert.ok(offset < tar.length, 'mutant must change an actual file header'); setArchive(gzipSync(tar));
    const result = await install(); assert.notEqual(result.status, 0); assert.match(result.stderr, /unsafe archive path/);
    assert.equal(existsSync(join(dir, 'escaped')), false); assert.equal(existsSync(join(home, 'escaped')), false);
    assert.deepEqual(readdirSync(home), []);
  });
});

test('uninstall preserves modified files and added content; an existing prefix is never overwritten', async () => {
  await fixture(async ({ home, prefix, install, uninstall }) => {
    assert.equal((await install()).status, 0);
    const before = readFileSync(join(prefix, 'bin/richview'));
    assert.notEqual((await install()).status, 0); assert.deepEqual(readFileSync(join(prefix, 'bin/richview')), before);
    for (const file of ['my-report.rmd', 'bin/.DS_Store']) {
      writeFileSync(join(prefix, file), 'keep this');
      const result = await uninstall(); assert.notEqual(result.status, 0); assert.match(result.stderr, /additional or missing content/);
      assert.ok(result.stderr.includes(`added: ./${file}`));
      assert.equal(readFileSync(join(prefix, file), 'utf8'), 'keep this'); rmSync(join(prefix, file));
    }
    rmSync(join(prefix, 'bin/richview'));
    let result = await uninstall(); assert.notEqual(result.status, 0); assert.match(result.stderr, /missing: .\/bin\/richview/);
    writeFileSync(join(prefix, 'bin/richview'), before); chmodSync(join(prefix, 'bin/richview'), 0o755);
    writeFileSync(join(prefix, 'notices/LICENSE'), 'local edit');
    result = await uninstall(); assert.notEqual(result.status, 0); assert.match(result.stderr, /file checksum mismatch/);
    assert.equal(readFileSync(join(prefix, 'notices/LICENSE'), 'utf8'), 'local edit');
    assert.match(result.stderr, /restore this file before retrying/);
    assert.deepEqual(readdirSync(home), ['.richview']);
  });
});

test('uninstall preserves reports, state and user PATH configuration and says what was kept', async () => {
  await fixture(async ({ home, prefix, install, uninstall }) => {
    const state = join(home, '.local/state/richview'); mkdirSync(state, { recursive: true });
    writeFileSync(join(state, 'snapshot'), 'saved state'); writeFileSync(join(home, 'report.rmd'), 'authored report');
    writeFileSync(join(home, '.profile'), `export PATH="${prefix}/bin:$PATH"\n`);
    assert.equal((await install()).status, 0);
    const result = await uninstall(); assert.equal(result.status, 0, JSON.stringify(result));
    assert.ok(result.stdout.includes(state)); assert.match(result.stdout, /SQL snapshots and upload journals/); assert.match(result.stdout, /Remove any PATH entry/);
    assert.equal(existsSync(prefix), false); assert.equal(readFileSync(join(state, 'snapshot'), 'utf8'), 'saved state');
    assert.equal(readFileSync(join(home, 'report.rmd'), 'utf8'), 'authored report');
    assert.equal(readFileSync(join(home, '.profile'), 'utf8'), `export PATH="${prefix}/bin:$PATH"\n`);
  });
});

test('empty and partial prefixes and scratch creation failures give safe recovery without deleting content', async () => {
  await fixture(async ({ home, prefix, install }) => {
    mkdirSync(prefix);
    for (const partial of [false, true]) {
      if (partial) writeFileSync(join(prefix, 'keep.txt'), 'keep');
      const attempts = [await install(), await run('sh', [installer, '--uninstall'], home)];
      for (const result of attempts) {
        assert.notEqual(result.status, 0); assert.match(result.stderr, /empty leftover/); assert.match(result.stderr, /rmdir/); assert.match(result.stderr, /move the directory aside/);
      }
      assert.ok(existsSync(prefix)); if (partial) assert.equal(readFileSync(join(prefix, 'keep.txt'), 'utf8'), 'keep');
      const recovery = attempts[0].stderr.split('\n').find(line => line.includes('For an empty leftover, run: ')).split('For an empty leftover, run: ')[1];
      const recovered = await run('sh', ['-c', recovery], home);
      if (partial) { assert.notEqual(recovered.status, 0); assert.equal(readFileSync(join(prefix, 'keep.txt'), 'utf8'), 'keep'); }
      else { assert.equal(recovered.status, 0); mkdirSync(prefix); }
    }
    const commands = join(home, 'commands'); mkdirSync(commands);
    writeFileSync(join(commands, 'mktemp'), '#!/bin/sh\nprintf "controlled mktemp permission denial\\n" >&2\nexit 1\n'); chmodSync(join(commands, 'mktemp'), 0o755);
    const result = await install(['--prefix', join(home, 'new-prefix')], { PATH: `${commands}:${process.env.PATH}` });
    assert.notEqual(result.status, 0); assert.match(result.stderr, /choose a writable --prefix under HOME/);
    assert.equal(existsSync(join(home, 'new-prefix')), false);
  });
});

test('latest release redirects to assets and download failures remain clear', async () => {
  await fixture(async ({ home, prefix, install, requests }) => {
    const result = await install(['--commit', '']);
    assert.equal(result.status, 0, JSON.stringify(result));
    assert.ok(existsSync(join(prefix, 'bin/richview')));
    assert.deepEqual(requests, [`/${archiveName}`, `/assets/${archiveName}`]);
  }, () => {}, false, { respond: (request, response) => {
    if (request.url.startsWith('/assets/')) return false;
    response.writeHead(302, { location: `/assets${request.url}` }); response.end(); return true;
  } });
  await fixture(async ({ home, install, requests }) => {
    const result = await install(); assert.notEqual(result.status, 0); assert.deepEqual(readdirSync(home), []);
    assert.match(result.stderr, /could not download archive/); assert.ok(result.stderr.includes(target));
    assert.equal(requests.length, 1);
  }, () => {}, false, { respond: (_, response) => { response.writeHead(404); response.end(); return true; } });
  await fixture(async ({ dir, home, install }) => {
    const commands = join(dir, 'commands'); mkdirSync(commands);
    writeFileSync(join(commands, 'curl'), '#!/bin/sh\nprintf "controlled curl timeout\\n" >&2\nexit 28\n'); chmodSync(join(commands, 'curl'), 0o755);
    const result = await install([], { PATH: `${commands}:${process.env.PATH}` });
    assert.notEqual(result.status, 0); assert.match(result.stderr, /could not download archive/); assert.match(result.stderr, /installation was not started/);
    assert.deepEqual(readdirSync(home), []);
  });
});

test('incorrect build identity and non-loopback HTTP are refused', async () => {
  await fixture(async ({ home, install }) => {
    const result = await install(['--commit', '2'.repeat(40)]); assert.notEqual(result.status, 0);
    assert.deepEqual(readdirSync(home), []);
  });
  await fixture(async ({ home, install }) => {
    const result = await install(); assert.notEqual(result.status, 0); assert.match(result.stderr, /archive commit\/platform differs/);
    assert.deepEqual(readdirSync(home), []);
  }, payload => {
    const build = `commit=${'2'.repeat(40)}\nplatform=${target}\n`;
    writeFileSync(join(payload, 'BUILD'), build);
    const sums = join(payload, 'SHA256SUMS');
    writeFileSync(sums, readFileSync(sums, 'utf8').replace(/^[0-9a-f]{64}  BUILD$/m, `${sha(build)}  BUILD`));
  });
  await fixture(async ({ home, install, requests }) => {
    const result = await install(['--base-url', 'http://127.0.0.1:80@example.invalid']);
    assert.notEqual(result.status, 0); assert.match(result.stderr, /HTTP downloads must stay on loopback/);
    assert.deepEqual(requests, []); assert.deepEqual(readdirSync(home), []);
  });
});

test('loopback redirects cannot change host even when the target also resolves locally', async () => {
  await fixture(async ({ home, install, requests }) => {
    const result = await install();
    assert.notEqual(result.status, 0); assert.match(result.stderr, /loopback redirect must stay on the same host/);
    assert.equal(requests.length, 1); assert.deepEqual(readdirSync(home), []);
  }, () => {}, false, { respond: (request, response) => {
    const port = request.headers.host.split(':').at(-1);
    response.writeHead(302, { location: `http://localhost:${port}/escaped` }); response.end(); return true;
  } });
});

test('Linux x64 without glibc is refused before downloading', async () => {
  await fixture(async ({ dir, home, install, requests }) => {
    const commands = join(dir, 'commands'); mkdirSync(commands);
    writeFileSync(join(commands, 'uname'), '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; esac\n');
    writeFileSync(join(commands, 'getconf'), '#!/bin/sh\nexit 1\n');
    chmodSync(join(commands, 'uname'), 0o755); chmodSync(join(commands, 'getconf'), 0o755);
    const result = await install([], { PATH: `${commands}:${process.env.PATH}` });
    assert.notEqual(result.status, 0); assert.match(result.stderr, /requires glibc/);
    assert.deepEqual(requests, []); assert.deepEqual(readdirSync(home), []);
  });
});

test('unsupported platforms fail before downloading', async () => {
  for (const [os, machine] of [['Darwin', 'x86_64'], ['Linux', 'aarch64'], ['MINGW64_NT', 'x86_64']]) {
    await fixture(async ({ dir, home, install, requests }) => {
      const commands = join(dir, 'commands'); mkdirSync(commands);
      writeFileSync(join(commands, 'uname'), `#!/bin/sh\ncase "$1" in -s) echo ${os};; -m) echo ${machine};; esac\n`);
      chmodSync(join(commands, 'uname'), 0o755);
      const result = await install([], { PATH: `${commands}:${process.env.PATH}` });
      assert.notEqual(result.status, 0); assert.match(result.stderr, /not supported yet/);
      assert.deepEqual(requests, []); assert.deepEqual(readdirSync(home), []);
    });
  }
});
