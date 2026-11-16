// Host reason (ARC-03): bound Chromium startup and drain only the launch's own processes before one relaunch.
import { AsyncLocalStorage } from 'node:async_hooks';
import { execFileSync } from 'node:child_process';
import { channel } from 'node:diagnostics_channel';
import { appendFileSync } from 'node:fs';
import { isAbsolute } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { waitForBenchmark } from './quiet.mjs';

const launches = new AsyncLocalStorage();
const spawning = channel('tracing:child_process.spawn:start');

async function drain(owned) {
  const pending = [...owned].filter(item => !item.closed || Number.isSafeInteger(item.child.pid));
  let timer, waiting = true;
  try {
    await Promise.race([
      (async () => {
        for (const item of pending) {
          if (!waiting) return;
          const pid = item.child.pid;
          if (!Number.isSafeInteger(pid) || pid <= 0) throw new Error('Cannot drain Chromium startup: no owned process id');
          try {
            if (process.platform === 'win32') {
              if (item.closed) throw new Error('Cannot prove an exited Chromium launcher tree drained on Windows');
              execFileSync('taskkill', ['/pid', String(pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true });
            }
            else process.kill(-pid, 'SIGKILL');
          } catch (error) {
            if (error.code === 'ESRCH') continue;
            if (error.code === 'EPERM' && process.platform !== 'win32') {
              // macOS can report EPERM for an unreaped dead leader. Await only our
              // child's exit, then still prove the entire group absent below.
              await item.reaped;
              if (!waiting) return;
              continue;
            }
            throw error;
          }
        }
        await Promise.all(pending.map(item => item.done));
        if (process.platform === 'win32') return;
        // A group can outlive its leader and all inherited pipes. Root close alone is insufficient.
        while (waiting) {
          const live = pending.some(item => {
            try { process.kill(-item.child.pid, 0); return true; }
            catch (error) {
              if (error.code === 'ESRCH') return false;
              // Other exited members can remain unreaped after their leader. EPERM
              // is still present/unknown; only a subsequent ESRCH permits completion.
              if (error.code === 'EPERM') return true;
              throw error;
            }
          });
          if (!live) return;
          await sleep(25);
        }
      })(),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Chromium startup process group did not drain after termination')), 5000); }),
    ]);
  } finally { waiting = false; clearTimeout(timer); }
}

function relaunch(timeoutMs, journal) {
  const line = JSON.stringify({ version: 1, event: 'browser-startup-relaunch', surface: 'playwright', engine: 'chromium', attempt: 2, maxAttempts: 2, timeoutMs });
  if (journal) appendFileSync(journal, line + '\n', { mode: 0o600 });
  console.error('RICHVIEW_BROWSER_RELAUNCH ' + line);
}

/** Keep browser use outside this promise: only Chromium's launch TimeoutError permits one relaunch. */
export async function launchBrowser(browserType, options = {}, { TimeoutError } = {}) {
  if (browserType.name() !== 'chromium') { await waitForBenchmark(); return browserType.launch(options); }
  if (typeof TimeoutError !== 'function' || !(TimeoutError.prototype instanceof Error)) throw new Error('Chromium startup requires the caller SDK public TimeoutError class');
  const timeout = Number.isFinite(options.timeout) && options.timeout > 0 ? Math.min(options.timeout, 15000) : 15000;
  const journal = process.env.RICHVIEW_BROWSER_EVENT_LOG;
  if (journal && !isAbsolute(journal)) throw new Error('RICHVIEW_BROWSER_EVENT_LOG must be an absolute path');
  for (let attempt = 1; attempt <= 2; attempt++) {
    await waitForBenchmark();
    if (attempt === 2) relaunch(timeout, journal);
    const owned = new Set();
    const observe = ({ process: child, options: spawnOptions }) => {
      // Playwright creates an isolated browser process with its two additional protocol pipes.
      // Node's public diagnostic gives the ChildProcess itself, without parsing log text or SDK internals.
      if (launches.getStore() !== owned || spawnOptions.stdio?.[3]?.type !== 'pipe' || spawnOptions.stdio?.[4]?.type !== 'pipe') return;
      if (process.platform !== 'win32' && !spawnOptions.detached) return;
      const item = { child, closed: false };
      item.reaped = new Promise(resolve => child.once('exit', resolve));
      item.done = new Promise(resolve => child.once('close', () => { item.closed = true; resolve(); }));
      owned.add(item);
    };
    spawning.subscribe(observe);
    try { return await launches.run(owned, () => browserType.launch({ ...options, timeout })); }
    catch (error) {
      await drain(owned);
      if (attempt === 2 || !(error instanceof TimeoutError)) throw error;
      if (![...owned].some(item => Number.isSafeInteger(item.child.pid))) throw new Error('Cannot relaunch Chromium without an observed owned launch process', { cause: error });
    } finally { spawning.unsubscribe(observe); }
  }
}
