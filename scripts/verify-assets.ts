// Upload, resume, verify and abort integrity-pinned assets against a local Garage store.
// Runs when GARAGE_BIN names a Garage 2.3 or later binary; every server binds to 127.0.0.1.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import type { ChildProcess, SpawnSyncReturns } from 'node:child_process';
import { createHash, createHmac, randomBytes, type KeyLike } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync, type PathOrFileDescriptor } from 'node:fs';
import http from 'node:http';
import { networkInterfaces } from 'node:os';
import { join, resolve } from 'node:path';

type Action = 'pass' | 'hold' | 'reset' | 'slowdown' | 'retype';
type WireEntry = { request: string; key: string; action?: Action; status?: number; listed_parts?: number[]; code?: string };
type Asset = { url: string; bytes: number; sha256: string; type: string; visibility: 'public' | 'private'; duration?: number; width?: number; height?: number };
type UploadEvent =
  | { event: 'progress'; sent: number; parts_done: number }
  | { event: 'retry'; part?: number; attempt: number; reason: string; wait_ms: number }
  | { event: 'start' | 'exists' | 'done' | 'restart' };
type CliResult<Output, Event> = { args: readonly string[]; status: number | null; signal: NodeJS.Signals | null; events: Event[]; output: Output };
type PutOutput = { asset: Asset; key: string; parts: number; part_size: number; sent_parts: number };
type PutRefusal = { code: string; error: string };
type VerifyOutput = { checked: { problems: string[] }[] };
type Journal = { key: string; upload_id: string; parts: { n: number }[] };
type McpRequest = { id?: number; method: string; params?: object };
type McpProgress = { progress: number };
type McpReply = { id?: number; result?: unknown; method?: string; params?: McpProgress };
type McpToolResult<T> = { isError: boolean; structuredContent: T };
type LocalResponse = { status?: number; headers: http.IncomingHttpHeaders; body: Buffer };

const garage = process.env.GARAGE_BIN;
if (!garage) {
  console.log(JSON.stringify({ skipped: true, reason: 'Set GARAGE_BIN to a Garage 2.3 or later binary to run the asset upload checks against a local store.' }));
  process.exit(0);
}
const root = resolve(new URL('..', import.meta.url).pathname);
const cli = join(root, 'target/release/richview');
const first = Number(process.env.ASSET_PORTS || 18740);
const ports = { s3: first, rpc: first + 1, web: first + 2, admin: first + 3, wire: first + 4 };
mkdirSync(join(root, 'tmp/verification'), { recursive: true });
const dir = mkdtempSync(join(root, 'tmp/verification/assets-'));
const state = join(dir, 'state');
const journals = join(state, 'richview/uploads');
const accessKey = 'GK' + randomBytes(12).toString('hex');
const secretKey = randomBytes(32).toString('hex');
const publicHost = `richview-public.web.garage.localhost:${ports.web}`;
const evidence: {
  garage: string;
  ports: typeof ports;
  network_interfaces: string[];
  steps: Record<string, object>;
  secret_scan?: { checked_bytes: number; credentials_checked: number; found: number };
  loopback_only?: { store_endpoints: string[]; public_base_url: string; isolated_network: boolean };
} = { garage: '', ports, network_interfaces: Object.keys(networkInterfaces()).sort(), steps: {} };
const transcript: { cli: CliResult<unknown, unknown>[]; wire: WireEntry[]; mcp: { requests: McpRequest[]; replies: McpReply[] } | [] } = { cli: [], wire: [], mcp: [] };

writeFileSync(join(dir, 'garage.toml'), `metadata_dir = "${dir}/garage/meta"
data_dir = "${dir}/garage/data"
db_engine = "sqlite"
replication_factor = 1
rpc_bind_addr = "127.0.0.1:${ports.rpc}"
rpc_public_addr = "127.0.0.1:${ports.rpc}"
rpc_secret = "${randomBytes(32).toString('hex')}"
[s3_api]
s3_region = "garage"
api_bind_addr = "127.0.0.1:${ports.s3}"
root_domain = ".s3.garage.localhost"
[s3_web]
bind_addr = "127.0.0.1:${ports.web}"
root_domain = ".web.garage.localhost"
[admin]
api_bind_addr = "127.0.0.1:${ports.admin}"
admin_token = "${randomBytes(32).toString('base64')}"
`);
const garageEnv = { ...process.env, GARAGE_CONFIG_FILE: join(dir, 'garage.toml'), GARAGE_DEFAULT_ACCESS_KEY: accessKey, GARAGE_DEFAULT_SECRET_KEY: secretKey, GARAGE_DEFAULT_BUCKET: 'richview-private', RUST_LOG: 'garage=warn' };
const stores = join(dir, 'stores.yaml');
const store = (endpoint: string) => `    endpoint: ${endpoint}\n    region: garage\n    bucket: richview-private\n    public_bucket: richview-public\n    public_base_url: http://${publicHost}/\n    credentials: env\n`;
writeFileSync(stores, `stores:\n  garage:\n${store(`http://127.0.0.1:${ports.s3}`)}  garage-wire:\n${store(`http://127.0.0.1:${ports.wire}`)}`);
const env: NodeJS.ProcessEnv = { ...process.env, RICHVIEW_STORES: stores, XDG_STATE_HOME: state, AWS_ACCESS_KEY_ID: accessKey, AWS_SECRET_ACCESS_KEY: secretKey };
for (const key of ['AWS_SESSION_TOKEN', 'AWS_PROFILE']) delete env[key];

// Deterministic inputs: a 1.3 MB dataset and synthetic media of fixed sizes.
function synthetic(name: string, bytes: number) {
  const data = Buffer.alloc(bytes);
  for (let block = 0; block * 32 < bytes; block++) createHash('sha256').update(`${name}:${block}`).digest().copy(data, block * 32);
  writeFileSync(join(dir, name), data);
  return join(dir, name);
}
const rows = ['ts,sensor,value'];
for (let i = 0; i < 36_000; i++) rows.push(`2026-10-${String(1 + (i % 30)).padStart(2, '0')}T${String(i % 24).padStart(2, '0')}:00:00Z,sensor-${'abc'[i % 3]},${(20 + Math.sin(i / 90) * 1.5).toFixed(3)}`);
writeFileSync(join(dir, 'dataset.csv'), rows.join('\n') + '\n');
const files = { csv: join(dir, 'dataset.csv'), video: synthetic('clip.mp4', 20_037_160), other: synthetic('draft.mp4', 17 * 2 ** 20), audio: synthetic('interview.mp3', 18 * 2 ** 20) };
const sha256 = (file: PathOrFileDescriptor) => createHash('sha256').update(readFileSync(file)).digest('hex');

// A request recorder between the CLI and Garage that can hold, reset, slow down or retype requests.
let plan: (entry: WireEntry) => Action | Promise<Action> = () => 'pass';
const held: http.IncomingMessage[] = [];
function label(method: string | undefined, url: URL) {
  const query = url.searchParams;
  if (query.has('partNumber')) return `${method} part ${query.get('partNumber')}`;
  if (query.has('uploads')) return method === 'POST' ? 'POST create-multipart' : `${method} list-uploads`;
  if (query.has('uploadId')) return ({ GET: 'GET list-parts', POST: 'POST complete', DELETE: 'DELETE abort' } as Record<string, string>)[method!];
  return `${method} object`;
}
const wire = http.createServer(async (request, response) => {
  const url = new URL(request.url!, 'http://wire');
  const entry: WireEntry = { request: label(request.method, url), key: decodeURIComponent(url.pathname) };
  entry.action = await plan(entry);
  transcript.wire.push(entry);
  if (entry.action === 'hold') return held.push(request);
  if (entry.action === 'reset') return request.socket.destroy();
  if (entry.action === 'slowdown') {
    request.resume();
    entry.status = 503;
    response.writeHead(503, { 'content-type': 'application/xml' });
    return response.end('<Error><Code>SlowDown</Code><Message>Please reduce your request rate.</Message></Error>');
  }
  const headers = { ...request.headers };
  if (entry.action === 'retype') headers['content-type'] = 'text/html';
  const upstream = http.request({ host: '127.0.0.1', port: ports.s3, method: request.method, path: request.url, headers }, reply => {
    entry.status = reply.statusCode;
    const chunks: Buffer[] = [];
    reply.on('data', chunk => chunks.push(chunk));
    reply.on('end', () => {
      const body = Buffer.concat(chunks);
      if (entry.request === 'GET list-parts') entry.listed_parts = [...body.toString().matchAll(/<PartNumber>(\d+)<\/PartNumber>/g)].map(match => Number(match[1]));
      if (reply.statusCode! >= 400) entry.code = body.toString().match(/<Code>([^<]+)<\/Code>/)?.[1];
      response.writeHead(reply.statusCode!, reply.headers);
      response.end(body);
    });
  });
  // A store may answer before reading the whole body (a refused signature); drain the rest.
  upstream.on('error', () => { request.unpipe(upstream); request.resume(); if (!entry.status) response.destroy(); });
  upstream.flushHeaders();
  request.pipe(upstream);
});

// Signed S3 requests for inspection only: object state, part lists and pending uploads.
const hmac = (key: KeyLike, text: string|NodeJS.ArrayBufferView<ArrayBufferLike>) => createHmac('sha256', key).update(text).digest();
const encode = (text: string|number|boolean) => encodeURIComponent(text).replace(/[!'()*]/g, c => '%' + c.charCodeAt(0).toString(16).toUpperCase());
function s3(method: string, path: string, query: Record<string, string> = {}) {
  const date = new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d+/, '');
  const scope = `${date.slice(0, 8)}/garage/s3/aws4_request`;
  const headers: Record<string, string> = { host: `127.0.0.1:${ports.s3}`, 'x-amz-content-sha256': createHash('sha256').update('').digest('hex'), 'x-amz-date': date };
  const names = Object.keys(headers).sort();
  const search = Object.keys(query).sort().map(key => `${encode(key)}=${encode(query[key])}`).join('&');
  const canonical = [method, path, search, names.map(name => `${name}:${headers[name]}\n`).join(''), names.join(';'), headers['x-amz-content-sha256']].join('\n');
  const signing = ['aws4_request', 's3', 'garage', date.slice(0, 8)].reduceRight<Buffer | string>((key, part) => hmac(key, part), 'AWS4' + secretKey);
  const signature = createHmac('sha256', signing).update(['AWS4-HMAC-SHA256', date, scope, createHash('sha256').update(canonical).digest('hex')].join('\n')).digest('hex');
  headers.authorization = `AWS4-HMAC-SHA256 Credential=${accessKey}/${scope}, SignedHeaders=${names.join(';')}, Signature=${signature}`;
  return fetchLocal(ports.s3, method, path + (search ? '?' + search : ''), headers);
}
function fetchLocal(port: number, method: string, path: string, headers: http.OutgoingHttpHeaders = {}) {
  return new Promise<LocalResponse>((accept, reject) => {
    const request = http.request({ host: '127.0.0.1', port, method, path, headers }, response => {
      const chunks: Buffer[] = [];
      response.on('data', chunk => chunks.push(chunk));
      response.on('end', () => accept({ status: response.statusCode, headers: response.headers, body: Buffer.concat(chunks) }));
    });
    request.on('error', reject);
    request.end();
  });
}
const pendingUploads = async () => {
  let count = 0;
  for (const bucket of ['richview-private', 'richview-public']) count += ((await s3('GET', `/${bucket}`, { uploads: '' })).body.toString().match(/<Upload>/g) || []).length;
  return count;
};
const journalFiles = () => existsSync(journals) ? readdirSync(journals).filter(name => name.endsWith('.json')) : [];
const readJournal = () => JSON.parse(readFileSync(join(journals, journalFiles()[0]), 'utf8')) as Journal;

// Runs the CLI; stderr carries JSON-lines events that `onEvent` may react to.
function richview<Output = string, Event = string>(args: readonly string[], onEvent: (event: Event, child: ChildProcess) => void = () => {}, credentials: NodeJS.ProcessEnv = {}) {
  return new Promise<CliResult<Output, Event>>(accept => {
    const child = spawn(cli, args, { cwd: root, env: { ...env, ...credentials }, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '', stderr = '';
    const events: Event[] = [];
    const json = args.includes('json');
    child.stdout.on('data', (chunk: string) => { stdout += chunk; });
    child.stderr.on('data', (chunk: string) => {
      stderr += chunk;
      const lines = stderr.split('\n');
      stderr = lines.pop()!;
      for (const line of lines) { const event = (json ? JSON.parse(line) : line) as Event; events.push(event); onEvent(event, child); }
    });
    child.on('close', (status, signal) => {
      const result = { args, status, signal, events, output: (json && stdout.trim() ? JSON.parse(stdout) : stdout) as Output };
      transcript.cli.push(result);
      accept(result);
    });
  });
}
const put = <Output = PutOutput>(file: string, options: string[], onEvent?: (event: UploadEvent, child: ChildProcess) => void, credentials?: NodeJS.ProcessEnv) => richview<Output, UploadEvent>(['asset', 'put', file, '--format', 'json', ...options], onEvent, credentials);
function monotonic(events: UploadEvent[]) {
  const sent = events.filter(event => event.event === 'progress').map(event => event.sent);
  assert.ok(sent.length > 0 && sent.every((value: number, i: number) => i === 0 || value >= sent[i - 1]), JSON.stringify(sent));
  return sent;
}
function reference(name: string, asset: Asset) {
  const path = join(dir, `${name}.json`);
  writeFileSync(path, JSON.stringify(asset, null, 2) + '\n');
  return path;
}
const until = async (check: () => boolean, what: string, timeout = 20_000) => {
  const start = Date.now();
  while (!check()) {
    if (Date.now() - start > timeout) throw new Error(`Timed out waiting for ${what}`);
    await new Promise(accept => setTimeout(accept, 25));
  }
};

const server = spawn(garage, ['server', '--single-node', '--default-bucket'], { env: garageEnv, stdio: ['ignore', 'ignore', 'pipe'] });
let garageLog = '';
server.stderr.on('data', chunk => { garageLog += chunk; });
process.on('exit', () => { if (server.exitCode === null) server.kill('SIGKILL'); });
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => process.exit(1));
try {
  evidence.garage = spawnSync(garage, ['--version'], { encoding: 'utf8' }).stdout.trim().split('\n')[0].replace(/ \[features.*$/, '');
  for (let attempt = 0; ; attempt++) {
    const health = await fetchLocal(ports.admin, 'GET', '/health').catch(() => ({ status: 0 }));
    if (health.status === 200 && server.exitCode === null) break;
    if (attempt > 200 || server.exitCode !== null) throw new Error(`Garage did not start: ${garageLog}`);
    await new Promise(accept => setTimeout(accept, 100));
  }
  for (const args of [['bucket', 'create', 'richview-public'], ['bucket', 'allow', '--read', '--write', '--owner', 'richview-public', '--key', accessKey], ['bucket', 'website', '--allow', 'richview-public']]) {
    const result: SpawnSyncReturns<string> = spawnSync(garage, args, { env: garageEnv, encoding: 'utf8', timeout: 30_000 });
    assert.equal(result.status, 0, result.stderr);
  }
  await new Promise<void>(accept => wire.listen(ports.wire, '127.0.0.1', accept));

  // A changed Content-Type breaks the signature, so the store refuses the PUT.
  plan = (entry: { request: string; }) => entry.request === 'PUT object' ? 'retype' : 'pass';
  const retyped = await put<PutRefusal>(files.csv, ['--to', 'garage-wire']);
  assert.equal(retyped.status, 3);
  assert.equal(retyped.output.code, 'asset.transfer');
  assert.match(retyped.output.error, /^PUT: 403 /);
  const refusedKey = `/richview-private/private/data/${sha256(files.csv).slice(0, 16)}/dataset.csv`;
  assert.equal((await s3('HEAD', refusedKey)).status, 404);
  evidence.steps.signed_type = { exit: retyped.status, error: retyped.output.error, wire: transcript.wire.map(({ request, action, status }) => ({ request, action, status })), object_after_refusal: 404 };

  // A store that refuses a PUT before reading its body is reported as a refusal, not a network error.
  const created = spawnSync(garage, ['key', 'create', 'read-only'], { env: garageEnv, encoding: 'utf8', timeout: 30_000 });
  const readOnly = { id: created.stdout.match(/Key ID:\s*(GK[0-9a-f]+)/)![1], secret: created.stdout.match(/Secret key:\s*([0-9a-f]{64})/)![1] };
  assert.equal(spawnSync(garage, ['bucket', 'allow', '--read', '--key', readOnly.id, 'richview-public'], { env: garageEnv, timeout: 30_000 }).status, 0);
  const restricted = await put<PutRefusal>(synthetic('poster.png', 6 * 2 ** 20), ['--to', 'garage', '--public'], undefined, { AWS_ACCESS_KEY_ID: readOnly.id, AWS_SECRET_ACCESS_KEY: readOnly.secret });
  assert.equal(restricted.status, 3);
  assert.match(restricted.output.error, /^PUT: 403 AccessDenied/);
  assert.ok(!JSON.stringify(restricted).includes(readOnly.secret) && !JSON.stringify(restricted).includes(readOnly.id));
  evidence.steps.refused_write = { exit: restricted.status, error: restricted.output.error, attempts: 1 + restricted.events.filter(event => event.event === 'retry').length, retry_reasons: restricted.events.filter(event => event.event === 'retry').map(event => event.reason) };

  // The dataset goes up in one signed PUT, privately.
  const csv = await put(files.csv, ['--to', 'garage']);
  assert.equal(csv.status, 0, JSON.stringify(csv));
  assert.deepEqual([csv.output.parts, csv.output.sent_parts, csv.output.asset.visibility], [1, 1, 'private']);
  assert.equal(csv.output.asset.sha256, sha256(files.csv));
  const stored = await s3('HEAD', new URL(csv.output.asset.url).pathname);
  assert.deepEqual([stored.headers['content-type'], stored.headers['x-amz-meta-sha256'], Number(stored.headers['content-length'])], ['text/csv', csv.output.asset.sha256, csv.output.asset.bytes]);
  const anonymous = await fetchLocal(ports.s3, 'GET', new URL(csv.output.asset.url).pathname);
  assert.equal(anonymous.status, 403);
  // Without --format json, a repeated upload sends nothing and prints the reference as YAML.
  const again = await richview(['asset', 'put', files.csv, '--to', 'garage']);
  assert.equal(again.status, 0);
  assert.deepEqual(Object.fromEntries(again.output.trim().split('\n').map((line: string) => line.split(': ')).map(([key, value]) => [key, key === 'bytes' ? Number(value) : value])), csv.output.asset);
  assert.match(again.events.join('\n'), /^Already stored at private\/data\//m);
  evidence.steps.dataset = { asset: csv.output.asset, key: csv.output.key, parts: 1, events: csv.events.map(event => event.event), stored: { type: stored.headers['content-type'], sha256: stored.headers['x-amz-meta-sha256'], cache_control: stored.headers['cache-control'] ?? null }, anonymous_get: anonymous.status, repeated_put: { stdout: again.output, stderr: again.events } };

  // The video is killed after part 1; the rerun sends only parts 2 and 3 despite a reset and a 503.
  transcript.wire.length = 0;
  plan = (entry: { request: string; }) => ['PUT part 2', 'PUT part 3'].includes(entry.request) ? 'hold' : 'pass';
  const killed = await put(files.video, ['--to', 'garage-wire', '--public'], (event, child) => { if (event.event === 'progress' && event.parts_done === 1) child.kill('SIGKILL'); });
  assert.equal(killed.signal, 'SIGKILL');
  held.splice(0).forEach(request => request.socket.destroy());
  const beforeResume = readJournal();
  assert.deepEqual(beforeResume.parts.map(part => part.n), [1]);
  const listedAfterKill = [...(await s3('GET', `/richview-public/${beforeResume.key}`, { uploadId: beforeResume.upload_id })).body.toString().matchAll(/<PartNumber>(\d+)<\/PartNumber>/g)].map(match => Number(match[1]));
  assert.deepEqual(listedAfterKill, [1]);
  const firstRun = transcript.wire.splice(0);
  let duringResume: Journal | undefined;
  const attempts: Record<string, number> = {};
  plan = async (entry: { request: string; }) => {
    attempts[entry.request] = (attempts[entry.request] || 0) + 1;
    if (entry.request === 'PUT part 2' && attempts[entry.request] === 1) return 'reset';
    if (entry.request === 'PUT part 3' && attempts[entry.request] === 1) return 'slowdown';
    if (entry.request === 'PUT part 3') {
      await until(() => readJournal().parts.length === 2, 'part 2 in the journal');
      duringResume = readJournal();
    }
    return 'pass';
  };
  const resumed = await put(files.video, ['--to', 'garage-wire', '--public']);
  assert.equal(resumed.status, 0, JSON.stringify(resumed));
  assert.deepEqual([resumed.output.parts, resumed.output.part_size, resumed.output.sent_parts], [3, 8 * 2 ** 20, 2]);
  const secondRun = transcript.wire.splice(0);
  assert.deepEqual(secondRun.find(entry => entry.request === 'GET list-parts')!.listed_parts, [1]);
  assert.deepEqual([...new Set(secondRun.filter(entry => entry.request.startsWith('PUT part')).map(entry => entry.request))].sort(), ['PUT part 2', 'PUT part 3']);
  assert.deepEqual(duringResume!.parts.map(part => part.n).sort(), [1, 2]);
  assert.deepEqual(journalFiles(), []);
  const sent = monotonic(resumed.events);
  assert.equal(sent.at(-1), 20_037_160);
  const retries = resumed.events.filter(event => event.event === 'retry');
  assert.deepEqual(retries.map(event => event.part).sort(), [2, 3]);
  const video = resumed.output.asset;
  assert.equal(video.sha256, sha256(files.video));
  assert.match(video.url, new RegExp(`^http://${publicHost}/public/video/${video.sha256.slice(0, 16)}/clip\\.mp4$`));
  const publicPath = new URL(video.url).pathname;
  const headPublic = await fetchLocal(ports.web, 'HEAD', publicPath, { host: publicHost });
  const range = await fetchLocal(ports.web, 'GET', publicPath, { host: publicHost, range: 'bytes=0-1023' });
  const whole = await fetchLocal(ports.web, 'GET', publicPath, { host: publicHost });
  assert.equal(headPublic.status, 200);
  assert.equal(headPublic.headers['cache-control'], 'public, max-age=31536000, immutable');
  assert.equal(range.status, 206);
  assert.equal(createHash('sha256').update(whole.body).digest('hex'), video.sha256);
  evidence.steps.resume = {
    first_run: { signal: killed.signal, wire: firstRun.map(({ request, action, status }) => ({ request, action, status })) },
    journal_before_resume: beforeResume, list_parts_after_kill: listedAfterKill,
    second_run: { wire: secondRun.map(({ request, action, status, listed_parts }) => ({ request, action, status, ...(listed_parts ? { listed_parts } : {}) })), retries: retries.map(({ part, attempt, reason, wait_ms }) => ({ part, attempt, reason, wait_ms })), progress_sent: sent, sent_parts: resumed.output.sent_parts },
    journal_during_resume: duringResume, journal_after_completion: 'removed',
    public_object: { url: video.url, head: { status: headPublic.status, type: headPublic.headers['content-type'], length: Number(headPublic.headers['content-length']), cache_control: headPublic.headers['cache-control'], accept_ranges: headPublic.headers['accept-ranges'] ?? null }, range: { status: range.status, content_range: range.headers['content-range'] }, sha256_of_download: 'match' },
  };

  // verify checks HEAD metadata and, with --hash, the streamed content.
  const videoRef = reference('video', video);
  const csvRef = reference('dataset', csv.output.asset);
  const verified = await richview<VerifyOutput>(['asset', 'verify', videoRef, '--hash', '--format', 'json']);
  const verifiedPrivate = await richview<VerifyOutput>(['asset', 'verify', csvRef, '--to', 'garage', '--hash', '--format', 'json']);
  assert.equal(verified.status, 0, JSON.stringify(verified.output));
  assert.equal(verifiedPrivate.status, 0, JSON.stringify(verifiedPrivate.output));
  const tampered = await richview<VerifyOutput>(['asset', 'verify', reference('tampered', { ...video, sha256: 'f'.repeat(64) }), '--hash', '--format', 'json']);
  const wrongType = await richview<VerifyOutput>(['asset', 'verify', reference('wrong-type', { ...video, type: 'video/webm' }), '--format', 'json']);
  assert.deepEqual([tampered.status, wrongType.status], [4, 4]);
  evidence.steps.verify = { public: verified.output.checked[0], private: verifiedPrivate.output.checked[0], tampered_hash: { exit: tampered.status, problems: tampered.output.checked[0].problems }, wrong_type: { exit: wrongType.status, problems: wrongType.output.checked[0].problems } };

  // abort removes an interrupted upload from the store and its journal.
  plan = (entry: { request: string; }) => ['PUT part 2', 'PUT part 3'].includes(entry.request) ? 'hold' : 'pass';
  const interrupted = await put(files.other, ['--to', 'garage-wire'], (event, child) => { if (event.event === 'progress' && event.parts_done === 1) child.kill('SIGKILL'); });
  held.splice(0).forEach(request => request.socket.destroy());
  assert.equal(interrupted.signal, 'SIGKILL');
  const pendingBefore = await pendingUploads();
  const aborted = await richview<{ aborted: object[] }>(['asset', 'abort', '--format', 'json']);
  assert.equal(aborted.status, 0);
  assert.deepEqual([pendingBefore, await pendingUploads(), journalFiles().length], [1, 0, 0]);
  evidence.steps.abort = { pending_uploads_before: pendingBefore, aborted: aborted.output.aborted, pending_uploads_after: 0, journals_after: 0 };

  // The MCP tools return the CLI's JSON and report upload progress.
  const requests: McpRequest[] = [
    { id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'verify-assets', version: '1' } } },
    { method: 'notifications/initialized' },
    { id: 2, method: 'tools/list' },
    { id: 3, method: 'tools/call', params: { name: 'richview_asset_put', arguments: { path: files.audio, store: 'garage' }, _meta: { progressToken: 'interview-upload' } } },
  ];
  const mcp = spawn(cli, ['mcp'], { cwd: root, env, stdio: ['pipe', 'pipe', 'inherit'] });
  const closed = new Promise(accept => mcp.on('close', accept));
  const lines: McpReply[] = [];
  let pending = '';
  const reply = <Result>(id: number) => until(() => lines.some(line => line.id === id), `MCP reply ${id}`, 60_000).then(() => lines.find(line => line.id === id) as McpReply & { result: Result });
  mcp.stdout.on('data', chunk => { pending += chunk; const parts = pending.split('\n'); pending = parts.pop()!; for (const part of parts) lines.push(JSON.parse(part)); });
  for (const request of requests) mcp.stdin.write(JSON.stringify({ jsonrpc: '2.0', ...request }) + '\n');
  const uploaded = (await reply<McpToolResult<PutOutput>>(3)).result;
  assert.equal(uploaded.isError, false, JSON.stringify(uploaded));
  const verifyRequest = { id: 4, method: 'tools/call', params: { name: 'richview_asset_verify', arguments: { asset: uploaded.structuredContent.asset, store: 'garage', hash: true } } };
  requests.push(verifyRequest);
  mcp.stdin.end(JSON.stringify({ jsonrpc: '2.0', ...verifyRequest }) + '\n');
  const checked = (await reply<McpToolResult<VerifyOutput>>(4)).result;
  await closed;
  const tools = (await reply<{ tools: { name: string }[] }>(2)).result.tools.map(tool => tool.name);
  const notifications = lines.filter(line => line.method === 'notifications/progress').map(line => line.params!);
  assert.ok(tools.includes('richview_asset_put') && tools.includes('richview_asset_verify'));
  assert.equal(checked.isError, false, JSON.stringify(checked));
  assert.ok(notifications.length >= 3 && notifications.every((note, i) => i === 0 || note.progress > notifications[i - 1].progress));
  assert.equal(notifications.at(-1)!.progress, 18 * 2 ** 20);
  transcript.mcp = { requests, replies: lines };
  evidence.steps.mcp = { tools, progress: notifications, put: uploaded.structuredContent, verify: checked.structuredContent.checked[0] };
  assert.equal(await pendingUploads(), 0);

  // No credential reaches output, journals, profiles or evidence.
  const outputs = JSON.stringify({ evidence, transcript }) + readFileSync(stores, 'utf8') + JSON.stringify(beforeResume) + JSON.stringify(duringResume);
  const credentials = [accessKey, secretKey, readOnly.id, readOnly.secret];
  evidence.secret_scan = { checked_bytes: outputs.length, credentials_checked: credentials.length, found: credentials.filter(value => outputs.includes(value)).length };
  assert.equal(evidence.secret_scan.found, 0);
  evidence.loopback_only = { store_endpoints: [`127.0.0.1:${ports.s3}`, `127.0.0.1:${ports.wire}`], public_base_url: `http://${publicHost}/`, isolated_network: evidence.network_interfaces.join() === 'lo' };
  writeFileSync(join(dir, 'transcript.json'), JSON.stringify(transcript, null, 2).replaceAll(dir, '<dir>') + '\n');
  writeFileSync(join(dir, 'assets.json'), JSON.stringify(evidence, null, 2).replaceAll(dir, '<dir>') + '\n');
  console.log(JSON.stringify({ ok: true, garage: evidence.garage, isolated_network: evidence.loopback_only.isolated_network, evidence_path: join(dir, 'assets.json'), transcript_path: join(dir, 'transcript.json') }, null, 2));
} finally {
  wire.close();
  wire.closeAllConnections?.();
  server.kill('SIGINT');
  await Promise.race([new Promise(accept => server.once('exit', accept)), new Promise(accept => setTimeout(accept, 5000))]);
  if (server.exitCode === null && server.signalCode === null) server.kill('SIGKILL');
}
