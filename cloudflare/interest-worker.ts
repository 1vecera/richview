// Host reason (ARC-03): the Cloudflare adapter connects the website to rate limiting and D1.
// This endpoint stores opt-in only. It has no mail provider, send path or subscriber read API.
type InterestBindings = {
  ASSETS?: { fetch(request: Request): Response | Promise<Response> };
  SIGNUP_RATE_LIMIT?: { limit(options: { key: string }): Promise<{ success: boolean }> };
  INTEREST_DB?: {
    prepare(sql: string): { bind(...values: string[]): { run(): Promise<{ success: boolean }> } };
  };
};
type InterestInput = { email?: unknown; consent?: unknown; website?: unknown };

const consentVersion = 'richview-updates-2026-10-05';
const maximumBody = 1024;
const reply = (status: number, body: { ok: boolean; message?: string }, extra: Record<string, string> = {}) => Response.json(body, { status, headers: {
  'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff', ...extra,
} });

async function boundedJson(request: Request): Promise<InterestInput | null> {
  if (!request.headers.get('content-type')?.toLowerCase().startsWith('application/json')) return null;
  if (Number(request.headers.get('content-length')) > maximumBody || !request.body) return null;
  const reader = request.body.getReader();
  let length = 0;
  const chunks: Uint8Array[] = [];
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > maximumBody) { await reader.cancel(); return null; }
      chunks.push(value);
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  } catch { return null; }
}

export default {
  async fetch(request: Request, env: InterestBindings) {
    const url = new URL(request.url);
    if (url.pathname !== '/api/interest') {
      // No subscriber listing/export endpoint. Other routes belong to static RichView assets.
      if (url.pathname.startsWith('/api/')) return reply(404, { ok: false, message: 'Not found.' });
      return env.ASSETS!.fetch(request);
    }
    if (request.method !== 'POST') return reply(405, { ok: false, message: 'Use the signup form.' }, { Allow: 'POST' });
    if (request.headers.get('origin') !== url.origin || request.headers.get('sec-fetch-site') === 'cross-site') {
      return reply(403, { ok: false, message: 'Please sign up from the RichView website.' });
    }
    if (!env.INTEREST_DB || !env.SIGNUP_RATE_LIMIT) return reply(503, { ok: false, message: 'Signup is not available yet. Your email has not been saved.' });
    try {
      // Trusted Cloudflare header; the key is ephemeral platform state, never a D1 column.
      const key = request.headers.get('cf-connecting-ip') || 'local-preview';
      const { success } = await env.SIGNUP_RATE_LIMIT.limit({ key: `richview-interest:${key}` });
      if (!success) return reply(429, { ok: false, message: 'Too many attempts. Please wait a minute and try again.' }, { 'Retry-After': '60' });
      const input = await boundedJson(request);
      const email = typeof input?.email === 'string' ? input.email.trim().toLowerCase() : '';
      if (input?.consent !== true || typeof input?.website !== 'string' || input.website !== '' ||
          email.length > 254 || !/^[\x21-\x7e]+$/.test(email) || !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email)) {
        return reply(400, { ok: false, message: 'Enter a valid email address and agree to RichView updates.' });
      }
      const result = await env.INTEREST_DB.prepare(
        'INSERT INTO subscribers (email, consented_at, consent_version) VALUES (?, ?, ?) ON CONFLICT(email) DO NOTHING'
      ).bind(email, new Date().toISOString(), consentVersion).run();
      if (!result.success) throw new Error('D1 write was not acknowledged');
      // Identical response for new and existing addresses, only after acknowledged storage.
      return reply(200, { ok: true });
    } catch {
      // Do not log input, email, IP or exception details to Worker logs.
      return reply(503, { ok: false, message: 'We could not confirm your email was saved. Please try again later.' });
    }
  },
};
