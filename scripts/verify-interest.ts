import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { writeFileSync, mkdirSync } from 'node:fs';
import worker from '../cloudflare/interest-worker.ts';

const base = process.env.RICHVIEW_PREVIEW_URL || 'http://127.0.0.1:8790';
assert.match(base, /^http:\/\/127\.0\.0\.1:\d+$/); // This check never targets a live subscription list.
const valid = { email: 'test@example.invalid', consent: true, website: '' };
const request = (body: unknown, extra: Record<string, string> = {}) => new Request(base + '/api/interest', { method: 'POST', headers: { origin: base, 'content-type': 'application/json', ...extra }, body: JSON.stringify(body) });
let writes = 0;
const stub = { ASSETS: { fetch: () => new Response('asset') }, SIGNUP_RATE_LIMIT: { limit: async () => ({ success: true }) }, INTEREST_DB: { prepare: () => ({ bind: () => ({ run: async () => { writes++; return { success: true }; } }) }) } };
assert.equal((await worker.fetch(request(valid), {})).status, 503);
assert.equal((await worker.fetch(request(valid), { ...stub, SIGNUP_RATE_LIMIT: { limit: async () => ({ success: false }) } })).status, 429);
assert.equal(writes, 0);
for (const body of [{ ...valid, consent: false }, { ...valid, consent: 'true' }, { ...valid, website: 'spam' }, { ...valid, email: 'bad' }, { ...valid, email: 'x'.repeat(300) + '@example.invalid' }]) assert.equal((await worker.fetch(request(body), stub)).status, 400);
assert.equal(writes, 0);
assert.equal((await worker.fetch(request(valid, { origin: 'https://other.example' }), stub)).status, 403);
assert.equal((await worker.fetch(new Request(base + '/api/interest'), stub)).status, 405);
const unbounded = new Request(base + '/api/interest', { method: 'POST', headers: { origin: base, 'content-type': 'application/json' }, body: ' '.repeat(2000) });
assert.equal((await worker.fetch(unbounded, stub)).status, 400);
assert.equal((await worker.fetch(request(valid), { ...stub, INTEREST_DB: { prepare: () => ({ bind: () => ({ run: async () => ({ success: false }) }) }) } })).status, 503);
assert.equal((await worker.fetch(request(valid), { ...stub, INTEREST_DB: { prepare: () => { throw Error('offline'); } } })).status, 503);
assert.equal((await worker.fetch(request(valid), stub)).status, 200);
assert.equal(writes, 1);

const sql = <Row>(command: string): { results: Row[] }[] => JSON.parse(execFileSync(process.execPath, ['node_modules/wrangler/bin/wrangler.js', 'd1', 'execute', 'richview-interest-local', '--local', '--config', 'wrangler.interest-preview.toml', '--persist-to', process.env.RICHVIEW_INTEREST_STATE || 'tmp/interest-state', '--command', command, '--json'], { encoding: 'utf8', env: { ...process.env, WRANGLER_SEND_METRICS: 'false' } }));
// Delete synthetic fixture rows only; retain any other isolated review signups.
sql("DELETE FROM subscribers WHERE email IN ('interest-storage@example.invalid', 'interest-browser@example.invalid')");
let address = 1;
const post = async (body: unknown, ip = `192.0.2.${address++}`) => fetch(base + '/api/interest', { method: 'POST', headers: { origin: base, 'content-type': 'application/json', 'cf-connecting-ip': ip }, body: JSON.stringify(body) });
const first = await post({ ...valid, email: ' INTEREST-STORAGE@EXAMPLE.INVALID ' });
assert.equal(first.status, 200);
assert.deepEqual(await first.json(), { ok: true });
const row = sql<{ consent_version: string; consented_at: string }>("SELECT * FROM subscribers WHERE email = 'interest-storage@example.invalid'")[0].results;
assert.equal(row.length, 1);
assert.equal(row[0].consent_version, 'richview-updates-2026-10-05');
assert.ok(Number.isFinite(Date.parse(row[0].consented_at)));
assert.equal((await post({ ...valid, email: 'interest-storage@example.invalid' })).status, 200);
assert.deepEqual(sql("SELECT * FROM subscribers WHERE email = 'interest-storage@example.invalid'")[0].results, row);
assert.equal((await post({ ...valid, consent: false })).status, 400);
assert.equal((await post({ ...valid, website: 'filled' })).status, 400);
assert.equal((await post({ ...valid, email: 'broken' })).status, 400);
assert.equal((await fetch(base + '/api/interest')).status, 405);
assert.equal((await fetch(base + '/api/subscribers')).status, 404);
let throttled = false;
for (let i = 0; i < 11; i++) { const r = await post({ ...valid, email: 'interest-storage@example.invalid' }, '192.0.2.199'); if (r.status === 429) { throttled = true; assert.equal(r.headers.get('retry-after'), '60'); break; } }
assert.ok(throttled);
const columns = sql<{ name: string }>('PRAGMA table_info(subscribers)')[0].results.map(c => c.name);
assert.deepEqual(columns, ['email', 'consented_at', 'consent_version']);
mkdirSync('tmp/interest-evidence', { recursive: true });
writeFileSync('tmp/interest-evidence/storage.json', JSON.stringify({ status: 'passed', scope: 'isolated local Cloudflare Workers runtime and D1', checks: ['acknowledged D1 insert', 'normalised deduplication preserves original consent', 'missing consent and honeypot rejected', 'bounded body', 'cross-origin rejected', 'storage failure never reports success', 'private schema has no IP/user-agent', 'subscriber read endpoint absent', 'ten/minute rate limiter returns 429'] }, null, 2));
console.log('Signup contract and actual local D1 storage checks passed.');
