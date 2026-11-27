// One real tutorial: source + local CSV/style -> native export -> offline interactive browser.
import { launchBrowser } from "../../tools/rv-gate/lib/browser-launch.mts";
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { Browser } from 'playwright';
import { chromium, firefox, webkit, errors as playwrightErrors } from 'playwright';
import { chromiumPath, root, watchErrors } from '../browser-harness.ts';

const disabled = process.argv.includes('--disable-runtime');
const engine = process.env.RICHVIEW_VERIFY_BROWSER || 'chromium';
assert.ok(engine === 'chromium' || engine === 'firefox' || engine === 'webkit');
mkdirSync(join(root, 'tmp/verification'), { recursive: true });
const dir = mkdtempSync(join(root, 'tmp/verification', disabled ? 'journey-regression-' : 'journey-'));
const report = join(dir, 'first-report');
cpSync(join(root, 'examples/first-report'), report, { recursive: true, filter: path => !path.includes('/Rendered') });
const source = join(report, 'report.rmd');
const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('RICHVIEW_')));
interface JourneyEvidence {
  status: string;
  engine: string;
  disabledRuntime: boolean;
  checks: { width: number; reducedMotion: string; overflow: number; settled: unknown; table: string[][]; accent: string; lastValue: string; screenshot: string }[];
  errors: string[];
  network: string[];
  source: string;
  sourceSha256: string;
  validation?: { ok: boolean };
  generated?: { interactive: boolean };
  resources?: Record<string, string>;
  outputSha256?: string;
  browser?: string;
  failure?: string;
}

const evidence: JourneyEvidence = { status: 'running', engine, disabledRuntime: disabled, checks: [], errors: [], network: [],
  source: 'examples/first-report/report.rmd', sourceSha256: createHash('sha256').update(readFileSync(source)).digest('hex') };
let browser: Browser | undefined;
try {
  const cli = join(root, 'target/release/richview');
  const run = <T>(action: string): T => JSON.parse(execFileSync(cli, [action, source,
    '--format', 'json'],
    { cwd: root, env, encoding: 'utf8', timeout: 60_000 }));
  evidence.validation = run<{ ok: boolean }>('validate');
  assert.equal(evidence.validation.ok, true);
  evidence.generated = run<{ interactive: boolean }>('generate');
  assert.equal(evidence.generated.interactive, true);
  assert.equal(readFileSync(join(report, 'Rendered/data/orders.csv'), 'utf8'), readFileSync(join(report, 'data/orders.csv'), 'utf8'));
  assert.equal(readFileSync(join(report, 'Rendered/report.rstyle.yaml'), 'utf8'), readFileSync(join(report, 'report.rstyle.yaml'), 'utf8'));
  evidence.resources = Object.fromEntries(['data/orders.csv', 'report.rstyle.yaml'].map(path =>
    [path, createHash('sha256').update(readFileSync(join(report, path))).digest('hex')]));
  const output = join(report, 'Rendered/report.html');
  evidence.outputSha256 = createHash('sha256').update(readFileSync(output)).digest('hex');
  browser = await launchBrowser({ chromium, firefox, webkit }[engine], { headless: true,
    ...(engine === 'chromium' ? { executablePath: chromiumPath(), args: ['--no-sandbox'] } : {}) }, { TimeoutError: playwrightErrors.TimeoutError });
  evidence.browser = browser!.version();
  for (const width of [320, 390, 1440]) {
    for (const reducedMotion of ['reduce', 'no-preference'] as const) {
      const context = await browser!.newContext({ viewport: { width, height: 900 }, reducedMotion, javaScriptEnabled: !disabled });
      try {
        const page = await context.newPage();
        watchErrors(page, evidence.errors);
        await page.route('**/*', route => {
          if (/^(file|data|blob):/.test(route.request().url())) return route.continue();
          evidence.network.push(route.request().url());
          return route.abort();
        });
        await page.goto(pathToFileURL(output).href, { waitUntil: 'load' });
        assert.match(await page.locator('body').innerText(), /Orders increased from 120 to 180/);
        try {
          await page.waitForFunction(() => Boolean(window.__richview), null, { timeout: disabled ? 1500 : 45_000 });
        } catch {
          throw new Error('HARNESS_RUNTIME_MISSING: static report exists, but its interactive initializer did not start');
        }
        const settled = await page.locator('body').evaluate(async () => {
          const result = await window.__richview!.settled();
          await document.fonts.ready;
          return { result, pending: window.__richview!.pending() };
        });
        assert.deepEqual(settled.pending, []);
        const table = await page.locator('.rv-table tbody tr').evaluateAll(rows =>
          rows.map(row => [...(row as HTMLTableRowElement).cells].map(cell => cell.textContent!.trim())));
        assert.deepEqual(table, [['Jul', '120'], ['Aug', '150'], ['Sep', '180']]);
        assert.match(await page.locator('.rv-table tfoot').innerText(), /450/);
        const accent = await page.locator('body').evaluate(body => getComputedStyle(body).getPropertyValue('--rv-accent').trim());
        // The report-local brand #A52A2A, as the style cascade writes it.
        assert.equal(accent, 'oklch(0.48061 0.15966 25.562)', 'The report-local style must reach the browser');
        const overflow = await page.evaluate(() => document.documentElement.scrollWidth - innerWidth);
        assert.ok(overflow <= 1, `${width}px report overflows by ${overflow}px`);
        assert.equal(await page.locator('.rv-chart__frame').count(), 1);
        await page.locator('.rv-chart__frame').focus();
        await page.keyboard.press('Home');
        const tip = page.locator('.rv-chart__readout:not([hidden])');
        await tip.waitFor();
        assert.match(await tip.innerText(), /120/);
        await page.keyboard.press('End');
        await page.waitForFunction(() => document.querySelector('.rv-chart__readout:not([hidden])')?.textContent!.includes('180'));
        const lastValue = await tip.innerText();
        await page.keyboard.press('Escape');
        await page.evaluate(() => scrollTo(0, 0));
        const screenshot = `report-${width}-${reducedMotion}.png`;
        await page.screenshot({ path: join(dir, screenshot), fullPage: true });
        evidence.checks.push({ width, reducedMotion, overflow, settled, table, accent, lastValue, screenshot });
      } finally {
        await context.close();
      }
    }
  }
  assert.deepEqual(evidence.errors, []);
  assert.deepEqual(evidence.network, [], 'The generated report must remain interactive offline');
  evidence.status = 'passed';
  console.log(`PASS source/CSV/style -> offline runtime -> exact keyboard values at 320/390/1440px: ${dir}`);
} catch (error) {
  evidence.status = 'failed';
  evidence.failure = (error as Error).message;
  console.error(error);
  process.exitCode = 1;
} finally {
  if (browser) await browser.close();
  writeFileSync(join(dir, 'journey.json'), JSON.stringify(evidence, null, 2) + '\n');
}
