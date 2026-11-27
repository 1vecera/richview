// Run the edge adapter and real signup checks without credentials or hosted resources.
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, openSync, closeSync } from 'node:fs';
mkdirSync('tmp', { recursive: true });
const state = mkdtempSync('tmp/cloudflare-review-');
const cli = 'node_modules/wrangler/bin/wrangler.js';
const env: NodeJS.ProcessEnv = { ...process.env, WRANGLER_SEND_METRICS: 'false', RICHVIEW_INTEREST_STATE: state };
const run = (args: string[]) => {
  const result = spawnSync(process.execPath, args, { env, stdio: 'inherit' });
  if (result.status !== 0) throw Error(`Local Cloudflare check failed: ${args[0]} (${result.status})`);
};
run([cli, 'd1', 'migrations', 'apply', 'richview-interest-local', '--local', '--config', 'wrangler.interest-preview.toml', '--persist-to', state]);
const log = state + '/worker.log';
const output = openSync(log, 'w');
const server = spawn(process.execPath, [cli, 'dev', '--local', '--config', 'wrangler.interest-preview.toml', '--persist-to', state, '--ip', '127.0.0.1', '--port', '0'], { env, stdio: ['ignore', output, output] });
closeSync(output);
try {
  let base;
  const started = Date.now();
  while (!base) {
    const text = readFileSync(log, 'utf8');
    base = text.match(/Ready on (http:\/\/127\.0\.0\.1:\d+)/)?.[1];
    if (server.exitCode !== null || Date.now() - started > 45000) throw Error('Local Wrangler startup failed: ' + text);
    if (!base) await new Promise(resolve => setTimeout(resolve, 100));
  }
  env.RICHVIEW_PREVIEW_URL = base;
  env.RICHVIEW_VERIFY_URL = base;
  run(['scripts/verify-interest.ts']);
} finally {
  const exited = new Promise(resolve => server.once('exit', resolve));
  server.kill('SIGINT');
  await Promise.race([exited, new Promise(resolve => setTimeout(resolve, 3000).unref())]);
  if (server.exitCode === null) server.kill('SIGKILL');
}
