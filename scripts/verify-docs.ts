// Exercise the published tutorial through real native/MCP/browser boundaries.
import assert from 'node:assert/strict';
import { spawn, spawnSync, type ChildProcessByStdio } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { cli, harness, inputValue, root, settledCharts, verificationShard } from './browser-harness.ts';
import { checkChartPage } from './docs-chart-checks.ts';
import { filterCases } from '../tools/rv-gate/lib/cases.mts';
import { siteModeExpectations } from './site-mode.ts';
import type { BrowserContext, Page } from 'playwright';
import type { Readable } from 'node:stream';

// The chart gallery has a source guide; only the format reference is generated separately.
const guidePages = readdirSync(join(root, 'docs/public')).filter(name => name.endsWith('.md')).map(name => name.slice(0, -3));
const docPages = guidePages.length + 1;
const acceptedFile = join(root, 'docs/public/charts/accepted.txt');
const accepted = existsSync(acceptedFile) ? readFileSync(acceptedFile, 'utf8').split('\n').map(line => line.trim()).filter(line => line && !line.startsWith('#')) : null;
const chartTypes = readdirSync(join(root, 'docs/public/charts')).filter(name => name.endsWith('.md')).map(name => name.slice(0, -3)).filter(type => !accepted || accepted.includes(type)).sort();
assert.ok(chartTypes.length, 'No public chart types to exercise; an empty accepted set is not docs interaction evidence');

// A flow chart's and a pivot table's page belong to their element, not to the chart gallery: the sidebar
// marks that element's entry on them and lists no chart types.
const elementPages: Record<string, string> = { 'charts/flow': '/docs/charts/flow', 'charts/pivot': '/docs/elements#tables' };

type Phase = 'native' | 'main' | 'desktop' | 'phone' | 'no-js';
const phases: Phase[] = ['native', 'main', 'desktop', 'phone', 'no-js'];
// A selector alone cannot turn the ordinary complete check into a partial pass.
const affected = process.env.RICHVIEW_VERIFY_PARENT === 'affected-docs';
const admitted = process.env.RICHVIEW_VERIFY_PARENT === 'docs' || affected;
const caseScope = affected ? filterCases(phases, { key: key => key, parent: 'affected-docs' }).scope : undefined;
if (affected && !caseScope) throw new Error('Affected docs dispatcher must name its cases');
const selection: { cases: Phase[]; shard: { index: number; count: number; cohort: string[]; selected: string[] } } = admitted ? verificationShard(phases, { key: (key: Phase) => key }) : {
  cases: phases,
  shard: { index: 0, count: 1, cohort: phases, selected: phases },
};
if (admitted) assert.equal(selection.shard.count, phases.length, 'Docs shards select one indivisible phase per child');
if (caseScope) assert.ok(selection.cases.every((key: string) => caseScope.selected.includes(key)), 'The assigned docs phase must be requested');
const outAt = process.argv.indexOf('--out');
const out = outAt < 0 ? undefined : process.argv[outAt + 1];
if (outAt >= 0 && (!out || out.startsWith('--'))) throw new Error('--out requires a result directory');
const snippet = readFileSync(join(root, 'docs/public/authoring.md'), 'utf8').match(/````rmd\n([\s\S]+?)\n````/)![1];
const release = JSON.parse(readFileSync(join(root, 'assets/install/release.json'), 'utf8'));
const installEnabled = release.enabled && process.env.RICHVIEW_DEV_PREVIEW !== '1';
const repositoryPublic = installEnabled || process.argv.includes('--repository-public');
// The release's Windows switch keeps or drops the Windows spans, as the build does (ARC-67).
const windowsShips = release.windows === true;
const h = await harness('docs');
const env = { ...process.env };
for (const key of ['RICHVIEW_TOKEN', 'RICHVIEW_SERVER', 'RICHVIEW_PUBLIC_URL']) delete env[key];
const report = join(h.dir, 'first-report');
cpSync(join(root, 'examples/first-report'), report, { recursive: true });
const file = join(report, 'report.rmd');
const original = readFileSync(file, 'utf8');
type CliResult = { ok: boolean; status: string; diagnostics: unknown[]; interactive: boolean; blocks: number };
type ChartPage = { width: number } & (Awaited<ReturnType<typeof checkChartPage>> | { type: string; ok: false; error: string });
const evidence: {
  cli: {
    validate?: CliResult;
    check?: CliResult;
    generate?: CliResult;
    inline_csv_json_and_copyable_source?: boolean;
    three_distinct_style_exports?: boolean;
    live_preview?: boolean;
  };
  mcp: Partial<{ tool_count: number; resource_count: number; render_validate_check_learn: boolean }>;
  publication: Partial<{ isolated_loopback: boolean; immutable_republish: boolean; new_version: boolean; retained_source: boolean }>;
  browser: Record<string, boolean | number | number[]>;
  chart_pages: ChartPage[];
  failures: Record<string, boolean>;
} = { cli: {}, mcp: {}, publication: {}, browser: {}, chart_pages: [], failures: {} };

function command(args: readonly string[], expected = 0) {
  const result = spawnSync(cli, args, { cwd: root, env, encoding: 'utf8', timeout: 30_000 });
  assert.equal(result.status, expected, result.stderr || result.stdout);
  return result.stdout;
}

function json(commandName: string, path = file, extra: string[] = []): CliResult {
  return JSON.parse(command([commandName, path, '--format', 'json', ...extra]));
}

// Match the build's substitutions before comparing copyable examples with the source.
function docsSource(source: string) {
  if (!repositoryPublic) source = source.replace(/<!-- repository-public:start -->[\s\S]*?<!-- repository-public:end -->/g, '');
  // The build expands this placeholder into a copyable shell block only for an enabled install.
  const nativeInstall = installEnabled ? '```sh\ncurl -fsSL https://github.com/1vecera/richview/releases/latest/download/install.sh | sh\nexport PATH=\"$HOME/.richview/bin:$PATH\"\nrichview --help\n```' : '';
  source = source.replaceAll('{{native-install}}', nativeInstall);
  const windowsInstall = installEnabled ? '```powershell\nirm https://github.com/1vecera/richview/releases/latest/download/install.ps1 | iex\n```' : '';
  source = source.replaceAll('{{windows-install}}', windowsInstall);
  source = source.replace(/<!-- windows-preview:start -->([\s\S]*?)<!-- windows-preview:end -->/g, (_, body) => windowsShips ? body : '');
  source = source.replace(/<!-- windows-unsupported:start -->([\s\S]*?)<!-- windows-unsupported:end -->/g, (_, body) => windowsShips ? '' : body);
  source = source.replaceAll('{{agent-prompt}}', siteModeExpectations(installEnabled, repositoryPublic).prompt);
  return source;
}

// Rich-block fences are drawn as live blocks, not as code (richview-core's RICH_TAGS). A `code`
// block is drawn as code: what a reader copies from it is its `text`.
const richTags = ['chart', 'table', 'big', 'metric', 'callout', 'expand', 'code', 'image', 'embed', 'audio', 'video', 'scrolly'];

// The `text: |` of a `code` block's YAML, as the block shows it.
function codeText(lines: string[]) {
  const at = lines.findIndex((line: string) => /^text:\s*\|-?\s*$/.test(line));
  if (at < 0) return undefined;
  const body = [];
  for (const line of lines.slice(at + 1)) {
    if (line && !line.startsWith('  ')) break;
    body.push(line.slice(2));
  }
  while (body.length && !body.at(-1)) body.pop();
  return body.join('\n') + '\n';
}

function codeExamples(source: string) {
  const examples = [];
  let fence;
  let tag;
  let rich = false;
  let lines = [];
  for (const line of source.split('\n')) {
    const match = line.match(/^(`{3,})([^`]*)$/);
    if (!fence && match) { fence = match[1]; tag = match[2].trim().split(/\s+/)[0]; rich = richTags.includes(tag); lines = []; }
    else if (fence && match && match[1].length >= fence.length && !match[2].trim()) {
      if (!rich) examples.push(lines.join('\n') + '\n');
      else if (tag === 'code' && codeText(lines)) examples.push(codeText(lines));
      fence = undefined;
    } else if (fence) lines.push(line);
  }
  return examples;
}

// Compare every authored and generated copy block in order.
function assertCopyableCode(actual: unknown, source: string, name: string) {
  assert.deepEqual(actual, codeExamples(docsSource(source)), `${name} changed copyable code`);
}

async function assertPageCopyableCode(page: Page, name: string) {
  const blocks = await page.locator('.rv-report__body pre code').allTextContents();
  if (name.startsWith('charts/')) {
    const reportPath = `/examples/${name}/report.rmd`;
    const link = page.locator('[data-copy-source] + a');
    assert.equal(await link.getAttribute('href'), `/playground?src=${reportPath}`, `${name} playground source`);
    const report = await page.request.get(new URL(reportPath, page.url()).href);
    assert.equal(report.status(), 200, reportPath);
    assert.equal(blocks.length, 1, `${name} must have one copyable report`);
    assert.equal(await page.locator('.rv-report__body pre code.language-rmd').count(), 1, `${name} Copy source target`);
    assert.deepEqual(Buffer.from(blocks[0], 'utf8'), await report.body(), `${name} copy differs from the packaged playground report`);
    return;
  }
  const file = name === 'reference' ? 'docs/RMD-SPEC.md' : `docs/public/${name}.md`;
  assertCopyableCode(blocks, readFileSync(join(root, file), 'utf8'), name);
}

async function startServer(binary: string, args: readonly string[], serverEnv = env) {
  const process = spawn(binary, args, { cwd: root, env: serverEnv, stdio: ['ignore', 'pipe', 'pipe'] });
  const base = await new Promise<string>((accept, reject) => {
    let output = '';
    const timeout = setTimeout(() => reject(new Error(`Preview did not start: ${output}`)), 30_000);
    process.once('error', error => { clearTimeout(timeout); reject(error); });
    process.once('exit', code => { clearTimeout(timeout); reject(new Error(`Server exited: ${code}`)); });
    for (const stream of [process.stdout, process.stderr]) stream.on('data', chunk => {
      output += chunk;
      const match = output.match(/RichView at (http:\/\/127\.0\.0\.1:\d+)/);
      if (match) { clearTimeout(timeout); accept(match[1]); }
    });
  });
  return { process, base };
}

async function stopServer(server: ChildProcessByStdio<null, Readable, Readable>) {
  if (server.exitCode !== null) return;
  const exited = new Promise(accept => server.once('exit', accept));
  server.kill('SIGINT');
  await Promise.race([exited, new Promise(accept => setTimeout(accept, 3_000))]);
  if (server.exitCode === null) server.kill('SIGKILL');
}

const errors: string[] = [];
const completed: Phase[] = [];
const results: { caseKey: Phase; ok: boolean; error?: string }[] = [];
const observations: ({ caseKey: string; ok: boolean; } | { caseKey: string; ok: boolean; count: number; })[] = [];
const phaseTimings: Partial<Record<Phase, number>> = {};
const navigationViews: string[] = [];
const staticViews: string[] = [];
const navigationWidths = [1440, 1024, 768, 390, 320];
const navigationPages = ['index', ...guidePages.filter(name => name !== 'index'), 'charts', 'reference', ...chartTypes.map(type => `charts/${type}`)];
const staticPages = ['index', ...guidePages.filter(name => name !== 'index')];
let navigationContext: BrowserContext | undefined;

async function nativePhase() {
  const editorGuide = readFileSync(join(root, 'docs/public/vscode.md'), 'utf8');
  const editorManifest = JSON.parse(readFileSync(join(root, 'packages/vscode/package.json'), 'utf8'));
  const editorReadme = readFileSync(join(root, 'packages/vscode/README.md'), 'utf8');
  for (const command of editorManifest.contributes.commands) {
    assert.ok(editorGuide.includes(command.title), `Missing exact editor command in the VS Code guide: ${command.title}`);
    if (command.command === 'richview.insertBlock') {
      assert.ok(editorReadme.includes(`**${command.title}**`), `Missing exact editor command in the extension README: ${command.title}`);
    }
  }
  observations.push({ caseKey: 'native/editor-commands', ok: true });
  const first = join(h.dir, 'hello');
  command(['init', first]);
  for (const action of ['validate', 'check', 'generate']) json(action, join(first, 'report.rmd'));
  observations.push({ caseKey: 'native/init', ok: true });
  // Check the actual staged downloads, including every generated chart example.
  // The SQL download needs a seeded DuckDB and is exercised by connectors_e2e.
  const publicRoot = process.env.RICHVIEW_VERIFY_PUBLIC_DIR || join(root, 'dist/web');
  const downloads = [join(publicRoot, 'docs/example/report.rmd')];
  for (const folder of ['docs/examples', 'docs/gallery', 'examples/charts']) {
    const parent = join(publicRoot, folder);
    for (const entry of readdirSync(parent, { withFileTypes: true })) {
      const candidate = join(parent, entry.name, 'report.rmd');
      if (entry.isDirectory() && existsSync(candidate) && !(folder === 'docs/examples' && entry.name === 'sql')) downloads.push(candidate);
    }
  }
  assert.ok(downloads.length > 1, 'No staged example downloads found');
  for (const path of downloads) for (const action of ['validate', 'check']) {
    const result = json(action, path);
    assert.equal(result.status, 'clean', `${action} ${path}: ${JSON.stringify(result.diagnostics)}`);
  }
  observations.push({ caseKey: 'native/downloadable-examples', ok: true, count: downloads.length });

  const copied = join(h.dir, 'copied-example');
  mkdirSync(copied);
  writeFileSync(join(copied, 'report.rmd'), snippet);
  for (const action of ['validate', 'check', 'generate']) json(action, join(copied, 'report.rmd'));
  observations.push({ caseKey: 'native/authoring-snippet', ok: true });
  for (const section of ['grammar', 'chart-contract', 'style', 'workflow', 'anti-slop', 'slides']) assert.ok(command(['learn', section]).length > 100);
  observations.push({ caseKey: 'native/learn', ok: true });
  for (const action of ['validate', 'check', 'generate'] as const) evidence.cli[action] = json(action);
  assert.equal(evidence.cli.validate!.ok, true);
  assert.equal(evidence.cli.generate!.interactive, true);
  assert.equal(evidence.cli.generate!.blocks, 4);
  const defaultHtml = readFileSync(join(report, 'Rendered/report.html'), 'utf8');
  assert.match(defaultHtml, /450/);
  assert.equal(readFileSync(join(report, 'Rendered/data/orders.csv'), 'utf8'), readFileSync(join(report, 'data/orders.csv'), 'utf8'));
  assert.equal(readFileSync(join(report, 'Rendered/report.rstyle.yaml'), 'utf8'), readFileSync(join(report, 'report.rstyle.yaml'), 'utf8'));
  observations.push({ caseKey: 'native/report-export', ok: true });

  const csvSource = original.replace(/data:\n  inline:[\s\S]+?\nencoding:/, 'data:\n  csv: data/orders.csv\nencoding:');
  writeFileSync(file, csvSource);
  for (const action of ['validate', 'check', 'generate']) json(action);
  observations.push({ caseKey: 'native/csv-data', ok: true });
  writeFileSync(join(report, 'data/orders.json'), JSON.stringify([{ month: 'Jul', orders: 120 }, { month: 'Aug', orders: 150 }, { month: 'Sep', orders: 180 }]));
  writeFileSync(file, csvSource.replace('csv: data/orders.csv', 'json: data/orders.json'));
  for (const action of ['validate', 'check', 'generate']) json(action);
  observations.push({ caseKey: 'native/json-data', ok: true });
  writeFileSync(file, original.replace('csv: data/orders.csv', 'csv: data/missing.csv'));
  const missing = command(['validate', file, '--format', 'json'], 1);
  assert.match(missing, /missing\.csv/);
  evidence.failures.missing_resource_detected = true;
  writeFileSync(file, original);
  json('validate');
  writeFileSync(join(report, 'backup.rmd'), original);
  assert.match(command(['generate', file, '--format', 'json'], 1), /one|multiple|exactly/i);
  rmSync(join(report, 'backup.rmd'));
  json('generate');
  evidence.failures.repaired_and_regenerated = true;
  evidence.cli.inline_csv_json_and_copyable_source = true;
  observations.push({ caseKey: 'native/failure-recovery', ok: true });

  const themeHtml: Record<string, string> = {};
  for (const style of ['scientific', 'sleek', 'brutalist']) {
    json('generate', file, ['--style', style, '-o', `${style}.html`]);
    themeHtml[style] = readFileSync(join(report, `Rendered/${style}.html`), 'utf8');
    assert.match(themeHtml[style], /Orders increased from 120 to 180/);
  }
  assert.equal(new Set(Object.values(themeHtml)).size, 3);
  evidence.cli.three_distinct_style_exports = true;
  assert.equal(readFileSync(file, 'utf8'), original);
  observations.push({ caseKey: 'native/style-exports', ok: true });

  const requests = [
    { id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'docs-check', version: '1' } } },
    { method: 'notifications/initialized' },
    { id: 2, method: 'tools/list' },
    { id: 3, method: 'resources/list' },
    { id: 4, method: 'tools/call', params: { name: 'richview_render', arguments: { inputPath: file, outputName: 'mcp.html' } } },
    { id: 5, method: 'tools/call', params: { name: 'richview_validate', arguments: { inputPath: file } } },
    { id: 6, method: 'tools/call', params: { name: 'richview_check', arguments: { inputPath: file, chartWidth: 720 } } },
    { id: 7, method: 'tools/call', params: { name: 'richview_learn', arguments: { section: 'grammar' } } },
  ];
  const mcp = spawnSync(cli, ['mcp'], { cwd: root, env, encoding: 'utf8', timeout: 30_000, input: requests.map(r => JSON.stringify({ jsonrpc: '2.0', ...r }) + '\n').join('') });
  assert.equal(mcp.status, 0, mcp.stderr);
  const replies: { result: { tools: { name: string }[]; resources: { uri: string }[]; isError?: boolean } }[] = mcp.stdout.trim().split('\n').map(line => JSON.parse(line));
  assert.deepEqual(replies[1].result.tools.map(t => t.name).sort(), ['richview_asset_put', 'richview_asset_verify', 'richview_check', 'richview_image_search', 'richview_image_use', 'richview_learn', 'richview_publish', 'richview_render', 'richview_validate']);
  assert.deepEqual(replies[2].result.resources.map(r => r.uri).sort(), ['richview://examples/hello-world', 'richview://spec/rmd']);
  for (const reply of replies.slice(3)) assert.equal(reply.result.isError, false);
  evidence.mcp = { tool_count: 9, resource_count: 2, render_validate_check_learn: true };
  observations.push({ caseKey: 'native/mcp', ok: true });

  const preview = await startServer(cli, ['serve', file, '--port', '0']);
  try {
    assert.match(await (await fetch(preview.base)).text(), /Orders increased from 120 to 180/);
    evidence.cli.live_preview = true;
  } finally { await stopServer(preview.process); }
  observations.push({ caseKey: 'native/live-preview', ok: true });

  const token = randomUUID();
  const native = await startServer(cli, ['server', '--port', '0', '--data-dir', join(h.dir, 'publish-store')], { ...env, RICHVIEW_TOKEN: token });
  try {
    const publish = (status = 0) => {
      const result = spawnSync(cli, ['publish', file, '--server', native.base, '--format', 'json'], { cwd: root, env: { ...env, RICHVIEW_TOKEN: token, RICHVIEW_SERVER: native.base }, encoding: 'utf8', timeout: 30_000 });
      assert.equal(result.status, status, result.stderr || result.stdout);
      return JSON.parse(result.stdout);
    };
    const firstVersion = publish();
    assert.equal(firstVersion.created, true);
    assert.equal(publish().created, false);
    // A published version is immutable: changed content under the same version is refused with a diagnostic.
    writeFileSync(file, original.replace('Three months, one clear trend', 'Changed source'));
    assert.equal(publish(1).diagnostics[0].code, 'publish.version-exists');
    // Reports are private until shared (FEA-SF-15), so the owner's token reads them.
    const owner = { headers: { authorization: `Bearer ${token}` } };
    assert.equal(await (await fetch(native.base + '/raw/first-report/v/0.0.1', owner)).text(), original);
    writeFileSync(file, original.replace('version: 0.0.1', 'version: 0.0.2'));
    assert.equal(publish().created, true);
    const versions = await (await fetch(native.base + '/api/reports/first-report/versions', owner)).json();
    assert.equal(versions.length, 2);
    evidence.publication = { isolated_loopback: true, immutable_republish: true, new_version: true, retained_source: true };
  } finally { writeFileSync(file, original); await stopServer(native.process); }
  observations.push({ caseKey: 'native/publication', ok: true });

}

async function startNavigation() {
  navigationContext = await h.browser.newContext();
  const page = await navigationContext.newPage();
  await page.emulateMedia({ reducedMotion: 'reduce' });
  page.on('pageerror', error => errors.push(error.message));
  const links = new Set<string>();
  for (const width of navigationWidths) {
    await page.setViewportSize({ width, height: 1000 });
    for (const name of navigationPages) {
      const response = await page.goto(`${h.base}/docs/${name === 'index' ? '' : name}`);
      assert.equal(response!.status(), 200);
      // Every page lists the galleries, the guides and the references, and under the elements gallery the
      // elements: the charts (the chart gallery's own entry), the flow chart's page and the parts of that
      // gallery's page. A chart page also lists the chart types, and the format reference its topic pages,
      // each in a list that opens.
      const menu = width < 980 ? '.docs-mobile-menu' : '.docs-sidebar';
      const elements = await page.locator(`${menu} [data-nav=elements] > a`).evaluateAll(nodes => nodes.map(node => node.getAttribute('href')));
      assert.ok(elements.includes('/docs/charts') && elements.length > 1, `${name} lists the elements: ${elements}`);
      const navLinks = docPages + 1 /* the Playground link */ + elements.length - 1 /* the chart gallery is a page already counted */;
      const inGallery = name.startsWith('charts') && !elementPages[name];
      const opened = async () => {
        const types = await page.locator(`${menu} .docs-chart-types[open] a:visible`).count();
        const topics = await page.locator(`${menu} [data-nav=topics][open] a:visible`).count();
        assert.equal(types, inGallery ? chartTypes.length - Object.keys(elementPages).length : 0, `${name} chart types listed`);
        assert.ok(name === 'reference' ? topics > 1 : topics === 0, `${name} lists ${topics} reference topics`);
        return types + topics;
      };
      await page.evaluate(() => document.fonts.ready);
      const activeLinks = await page.locator('.docs-sidebar .docs-nav a[aria-current]').evaluateAll(nodes => nodes.map(node => node.getAttribute('href')));
      const current = name === 'index' ? '/docs/' : `/docs/${name}`;
      const active = elementPages[name] ? [elementPages[name]] : name.startsWith('charts/') ? ['/docs/charts', current] : [current];
      assert.deepEqual(activeLinks, active, `${name} active navigation`);
      if (name === 'charts') {
        // On the gallery the chart types list moves the reader within the page: every entry names a chart on it.
        const types = await page.locator(`${menu} .docs-chart-types a`).evaluateAll(nodes => nodes.map(node => node.getAttribute('href')));
        for (const href of types) assert.equal(href!.startsWith('#') ? await page.locator(`[id="${href!.slice(1)}"]`).count() : 0, 1, `Chart types entry ${href} is not on the gallery page`);
      }
      if (name === 'elements') {
        for (const href of elements.filter(href => href!.startsWith('/docs/elements#'))) assert.equal(await page.locator(`[id="${href!.split('#')[1]}"]`).count(), 1, `Broken element link ${href}`);
      }
      const icon = await page.locator('link[rel=icon]').getAttribute('href');
      assert.equal(icon, '/landing/favicon.svg');
      assert.equal((await page.request.get(h.base + icon)).status(), 200);
      assert.ok(await page.locator('h1').textContent());
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), `${name} overflows at ${width}`);
      assert.match(await page.locator('.rv-report__body').evaluate(node => getComputedStyle(node).fontFamily), /^Geist\b/);
      assert.ok(await page.evaluate(() => document.fonts.check('16px Geist')), `${name} does not load Geist`);
      if (width < 980) {
        await page.locator('.docs-mobile-menu > summary').click();
        if (inGallery) await page.locator('.docs-mobile-menu .docs-chart-types summary').click();
        assert.equal(await page.locator('.docs-mobile-menu nav a:visible').count(), navLinks + await opened());
        assert.equal(await page.locator('.docs-mobile-menu a[aria-current]:visible').count(), active.length);
        await page.locator('.docs-mobile-menu > summary').click();
      } else {
        if (inGallery) await page.locator('.docs-sidebar .docs-chart-types summary').click();
        assert.equal(await page.locator('.docs-nav a:visible').count(), navLinks + await opened());
      }
      for (const href of await page.locator('.docs-toc nav a').evaluateAll(nodes => nodes.map(node => node.getAttribute('href')))) {
        // An id may start with a digit ("2-d-bin-chart"), which a `#id` CSS selector cannot name.
        assert.equal(await page.locator(`[id="${decodeURIComponent(href!.slice(1))}"]`).count(), 1, `Broken section link ${name}${href}`);
      }
      if (width === 1440) {
        await assertPageCopyableCode(page, name);
        const codeAudit = await page.locator('.docs-code pre').evaluateAll(nodes => {
          const painter = new OffscreenCanvas(1, 1).getContext('2d', { willReadFrequently: true });
          const luminance = (color: string|CanvasGradient|CanvasPattern) => {
            painter!.fillStyle = color; painter!.fillRect(0, 0, 1, 1);
            const rgb = [...painter!.getImageData(0, 0, 1, 1).data].slice(0, 3).map(v => v / 255).map(v => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4);
            return rgb[0] * .2126 + rgb[1] * .7152 + rgb[2] * .0722;
          };
          return nodes.map(pre => {
            // The code panel's own background sits on a wrapper when the pre is transparent.
            let surface = pre;
            while (surface.parentElement && /^rgba\(.*, 0\)$/.test(getComputedStyle(surface).backgroundColor)) surface = surface.parentElement;
            const background = luminance(getComputedStyle(surface).backgroundColor);
            const spans = [...pre.querySelectorAll('span')].filter(span => span.textContent.trim());
            const colors = spans.map(span => getComputedStyle(span).color);
            // Some RMD examples contain only prose constructs the highlighter does not mark.
            // Check their ordinary text contrast too, rather than treating an empty span set as a pass.
            const textColors = colors.length ? colors : [getComputedStyle(pre.querySelector('code')!).color];
            const contrast = textColors.map(color => { const foreground = luminance(color); return (Math.max(background, foreground) + .05) / (Math.min(background, foreground) + .05); });
            return { language: pre.querySelector('code')!.className, source: pre.textContent, colors: new Set(colors).size, contrast: Math.min(...contrast), fontSize: parseFloat(getComputedStyle(pre).fontSize) };
          });
        });
        for (const code of codeAudit) {
          assert.ok(code.contrast >= 4.5, `${name} syntax contrast ${JSON.stringify(code)}`);
          // The highlighter knows Rich Markdown, YAML and shell; JSON and plain-text blocks stay one colour.
          const plainRmd = code.language === 'language-rmd' && code.colors === 0
            && !/^(?:---\s*$|\s*(?:#{1,6} |[-*+>] |\d+\. |`{3}|~{3}))|\\.|`[^`]+`|\*\*\S[^\n]*?\*\*|__\S[^\n]*?__|(?:^|[^\w])(?:\*\S[^\n]*?\*|_\S[^\n]*?_)|\[[^\]\n]+\]\([^\n)]+\)/m.test(code.source);
          if (['language-rmd', 'language-yaml', 'language-sh', 'language-bash'].includes(code.language)) assert.ok(plainRmd || code.colors > 1, `${name} missing syntax colors ${JSON.stringify(code)}`);
          assert.ok(code.fontSize >= 12, `${name} code is too small`);
        }
      }
      if (name === 'charts') {
        await settledCharts(page, 1);
        const id = await page.evaluate(() => window.__richview!.charts().find(chart => chart.geometry.kind === 'dumbbell')?.figure);
        assert.ok(id, 'The chart gallery needs a dumbbell readout to check its row spacing');
        const figure = page.locator(`[id="${id}"]`);
        await figure.locator('.rv-chart__frame').focus();
        await page.keyboard.press('Home');
        const rows = figure.locator('.rv-chart__readout:not([hidden]) p');
        await rows.first().waitFor();
        const margins = await rows.evaluateAll(nodes => nodes.map(node => getComputedStyle(node).marginBottom));
        assert.ok(margins.length > 1 && margins.every(margin => margin === '0px'), `Gallery readout row margins: ${margins}`);
      }
      for (const url of await page.locator('a[href]').evaluateAll(nodes => nodes.map(node => (node as HTMLAnchorElement).href))) if (url.startsWith(h.base)) links.add(url);
      if (name === 'index') {
        const slides = page.locator(`${menu} nav a[href="/docs/slides"]`);
        assert.equal(await slides.count(), 1);
        assert.equal(await slides.evaluate(link => {
          let group = link.previousElementSibling;
          while (group && !group.matches('.docs-nav__group')) group = group.previousElementSibling;
          return group?.textContent;
        }), 'How to use');
        assert.equal(await page.locator('.rv-report__body a[href*="/docs/slides"], .rv-report__body iframe[src*="/warmest-years/"]').count(), 0);
        await page.screenshot({ path: join(h.dir, `docs-${width}.png`), fullPage: true });
        await page.screenshot({ path: join(h.dir, `docs-top-${width}.png`) });
        await page.locator('.docs-report-grid').scrollIntoViewIfNeeded();
        await page.screenshot({ path: join(h.dir, `docs-gallery-${width}.png`) });
      }
      navigationViews.push(`${width}:${name}`);
      observations.push({ caseKey: `main/navigation/${width}/${name}`, ok: true });
    }
  }
  return { page, links };
}

async function chartPhase(width: number) {
  const start = evidence.chart_pages.length;
  const phaseErrors = [];
  const touch = width === 390;
  const context = await h.browser.newContext({ viewport: { width, height: 900 }, hasTouch: touch, isMobile: touch, reducedMotion: 'reduce' });
  try {
    const chartPage = await context.newPage();
    chartPage.setDefaultTimeout(5000);
    chartPage.on('pageerror', error => { phaseErrors.push(error.message); errors.push(error.message); });
    for (const type of chartTypes) {
      const caseKey = `${touch ? 'phone' : 'desktop'}/${type}`;
      try {
        assert.equal((await chartPage.goto(`${h.base}/docs/charts/${type}`))!.status(), 200);
        evidence.chart_pages.push({ width, ...await checkChartPage(chartPage, type, { touch }) });
        observations.push({ caseKey, ok: true });
      } catch (error) {
        evidence.chart_pages.push({ width, type, ok: false, error: (error as Error).message });
        observations.push({ caseKey, ok: false });
      }
    }
  } finally { await context.close(); }

  return { ok: phaseErrors.length === 0 && evidence.chart_pages.slice(start).every(result => result.ok) };
}

async function staticPhase() {
  const noScript = await h.browser.newContext({ javaScriptEnabled: false });
  try {
    const staticPage = await noScript.newPage();
    for (const name of staticPages) {
      assert.equal((await staticPage.goto(`${h.base}/docs/${name === 'index' ? '' : name}`))!.status(), 200);
      const supported = staticPage.locator('.docs-code pre code:is(.language-rmd, .language-yaml, .language-sh, .language-bash)');
      if (await supported.count()) assert.ok(await supported.locator('span').count() > 0, `${name} requires JavaScript for highlighting`);
      if (name === 'authoring') {
        assert.ok(await supported.count() > 0, 'The known authoring highlighting fixture is missing');
        assert.ok(await supported.locator('span').count() > 0, 'The authoring fixture is not highlighted without JavaScript');
      }
      staticViews.push(name);
      observations.push({ caseKey: `no-js/${name}`, ok: true });
    }
  } finally { await noScript.close(); }
  evidence.browser.static_syntax_highlighting = true;
}

async function finishNavigation({ page, links }: Awaited<ReturnType<typeof startNavigation>>) {
  for (const url of links) assert.equal((await page.request.get(url)).status(), 200, url);
  observations.push({ caseKey: 'main/internal-links', ok: true });
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto(`${h.base}/playground`);
  // The playground preview's charts mount: verify-site proves the preview CSP lets the runtime load,
  // so a preview that never starts is a failure here too (review 1d note 11).
  const previewElement = await page.locator('#preview').elementHandle({ timeout: 45_000 });
  const pgPreview = await previewElement.contentFrame();
  assert.ok(pgPreview, 'the playground preview frame is attached');
  await previewElement.dispose();
  const previewChart = pgPreview.locator('figure[data-rv-chart]');
  const previewReady = async (expectedText?: string) => {
    if (expectedText) await pgPreview.locator('body').filter({ hasText: expectedText }).waitFor({ state: 'visible', timeout: 45_000 });
    await previewChart.first().waitFor({ state: 'attached', timeout: 45_000 });
    await pgPreview.locator('html[data-rv-runtime="started"]').waitFor({ state: 'attached', timeout: 45_000 });
    assert.equal(await pgPreview.locator('html').getAttribute('data-rv-runtime'), 'started', 'the playground preview runtime starts');
    await settledCharts(pgPreview, 1);
    assert.equal(await pgPreview.locator('figure[data-rv-chart] canvas.rv-chart__canvas').count(), 1);
  };
  await previewReady();
  await inputValue(page.locator('#source'), snippet);
  await page.waitForFunction(() => document.querySelector('#preview')!.getAttribute('srcdoc')!.includes('Orders increased from 120 to 180'));
  await previewReady('Orders increased from 120 to 180');
  assert.equal(await previewChart.count(), 1);
  observations.push({ caseKey: 'main/playground', ok: true });
  const computed: Record<string, { background: string; font: string; palette: number[]; rgb: number[]; barFill: string; data: unknown }> = {};
  for (const style of ['report', 'scientific', 'sleek', 'brutalist']) {
    await page.goto(`${h.base}/docs/example/Rendered/${style}.html`);
    await page.evaluate(() => document.fonts.ready);
    await settledCharts(page, 1);
    computed[style] = await page.evaluate(() => {
      const payload = JSON.parse(document.querySelector('script[data-rv-payload]')!.textContent);
      const [r, g, b] = window.__richview!.charts()[0].geometry.series[0].color;
      return {
        // The page colour, read from the style: a page background (STY-74) leaves the body transparent.
        background: getComputedStyle(document.documentElement).getPropertyValue('--rv-bg').trim(),
        font: getComputedStyle(document.querySelector('.rv-report__body')!).fontFamily,
        palette: (() => {
          const painter = new OffscreenCanvas(1, 1).getContext('2d', { willReadFrequently: true });
          painter!.fillStyle = payload.theme.palette[0];
          painter!.fillRect(0, 0, 1, 1);
          return [...painter!.getImageData(0, 0, 1, 1).data].slice(0, 3);
        })(),
        rgb: [r, g, b].map(v => Math.round(v * 255)),
        barFill: `rgb(${[r, g, b].map(v => Math.round(v * 255)).join(', ')})`,
        data: { columns: payload.columns, rows: payload.rows },
      };
    });
    await page.screenshot({ path: join(h.dir, `example-${style}.png`), fullPage: true });
  }
  assert.equal(new Set(['scientific', 'sleek', 'brutalist'].map(style => computed[style].font)).size, 3);
  assert.equal(new Set(['scientific', 'sleek', 'brutalist'].map(style => computed[style].barFill)).size, 3, JSON.stringify(Object.fromEntries(Object.entries(computed).map(([style, value]) => [style, { palette: value.palette, barFill: value.barFill }]))));
  for (const style of ['scientific', 'sleek', 'brutalist']) assert.deepEqual(computed[style].data, computed.report.data);
  // The drawn first series is the theme's first palette colour (rounding may differ by one).
  for (const style of ['report', 'scientific', 'sleek', 'brutalist']) {
    computed[style].palette.forEach((v, i) => assert.ok(Math.abs(v - computed[style].rgb[i]) <= 1, `${style} series colour ${computed[style].rgb} vs theme ${computed[style].palette}`));
  }
  assert.notEqual(computed.report.background, computed.scientific.background);
  observations.push({ caseKey: 'main/style-contract', ok: true });
  assert.equal(errors.length, 0, errors.join('\n'));
  evidence.browser = { ...evidence.browser, desktop_and_mobile: true, viewports: [1440, 1024, 768, 390, 320], internal_links: links.size, runtime_errors: errors.length, distinct_fonts_and_rendered_bar_colors: true, source_values_unchanged: true, local_yaml_changes_background: true, code_text_matches_source: true, syntax_contrast_at_least_4_5: true, mobile_navigation: true, section_links: true };
}

async function runPhase(key: Phase, action: () => Promise<void | { ok: boolean }>, prefixMs = 0) {
  const started = Date.now();
  try {
    const outcome = await action();
    const ok = outcome?.ok !== false;
    results.push({ caseKey: key, ok });
    if (ok) completed.push(key);
  } catch (error) {
    results.push({ caseKey: key, ok: false, error: (error as Error).message });
    throw error;
  } finally { phaseTimings[key] = prefixMs + Date.now() - started; }
}

const chartKey = (result: { width: number; type: string }) => result.width + ':' + result.type;
const expectedChartCohort = [1440, 390].flatMap(width => chartTypes.map(type => width + ':' + type));
const observationCohorts = {
  native: ['editor-commands', 'init', 'downloadable-examples', 'authoring-snippet', 'learn', 'report-export', 'csv-data', 'json-data', 'failure-recovery', 'style-exports', 'mcp', 'live-preview', 'publication'].map(key => `native/${key}`),
  main: [...navigationWidths.flatMap(width => navigationPages.map(name => `main/navigation/${width}/${name}`)), 'main/internal-links', 'main/playground', 'main/style-contract'],
  desktop: chartTypes.map(type => `desktop/${type}`),
  phone: chartTypes.map(type => `phone/${type}`),
  'no-js': staticPages.map(name => `no-js/${name}`),
};
const checkCharts = () => {
  const widths = selection.cases.filter((key: string) => key === 'desktop' || key === 'phone').map((key: string) => key === 'desktop' ? 1440 : 390);
  const expected = widths.flatMap((width: number) => chartTypes.map(type => width + ':' + type)).sort();
  assert.deepEqual(evidence.chart_pages.map(chartKey).sort(), expected, 'Every selected docs chart page must be exercised exactly once');
  assert.deepEqual(evidence.chart_pages.filter(result => !result.ok), [], 'docs chart interactions failed (see chart-pages.json)');
};
let failure;
try {
  if (selection.shard.count === 1) {
    // Preserve the complete original main-page navigation and viewport lifecycle, including its
    // position around the chart and script-disabled contexts. Shards isolate those contexts.
    await runPhase('native', nativePhase);
    let journey;
    const started = Date.now();
    try { journey = await startNavigation(); }
    catch (error) { results.push({ caseKey: 'main', ok: false, error: (error as Error).message }); throw error; }
    const prefixMs = Date.now() - started;
    await runPhase('desktop', () => chartPhase(1440));
    await runPhase('phone', () => chartPhase(390));
    h.save('chart-pages.json', evidence.chart_pages);
    checkCharts();
    await runPhase('no-js', staticPhase);
    await runPhase('main', () => finishNavigation(journey), prefixMs);
  } else {
    const [key] = selection.cases;
    if (key === 'native') await runPhase(key, nativePhase);
    else if (key === 'main') await runPhase(key, async () => finishNavigation(await startNavigation()));
    else if (key === 'no-js') await runPhase(key, staticPhase);
    else await runPhase(key, () => chartPhase(key === 'desktop' ? 1440 : 390));
    checkCharts();
  }
  assert.equal(errors.length, 0, errors.join('\n'));
  assert.deepEqual(navigationViews, selection.cases.includes('main') ? navigationWidths.flatMap(width => navigationPages.map(name => `${width}:${name}`)) : [], 'Every selected docs navigation view must complete all assertions');
  assert.deepEqual(staticViews, selection.cases.includes('no-js') ? staticPages : [], 'Every selected static docs page must complete all assertions');
  assert.deepEqual(observations, observations.filter(result => result.ok), 'Every observed docs journey must pass');
  assert.deepEqual(observations.map(result => result.caseKey).sort(), selection.cases.flatMap(phase => observationCohorts[phase]).sort(), 'Every selected docs journey must complete exactly once');
  assert.deepEqual([...completed].sort(), [...selection.shard.selected].sort(), 'Every selected docs phase must finish all assertions');
} catch (error) { failure = error; throw error; }
finally {
  try {
    const suffix = selection.shard.count === 1 ? '' : '-' + selection.shard.index + '-of-' + selection.shard.count;
    h.save('chart-pages' + suffix + '.json', evidence.chart_pages);
    const report = { ...evidence, ok: !failure, admission: admitted ? 'docs' : null, engine: h.engine, browser_version: h.browser.version(), shard: selection.shard, ...(caseScope && { case_scope: caseScope }), completed, results: results.sort((a, b) => phases.indexOf(a.caseKey) - phases.indexOf(b.caseKey)), observations, errors, chart_cohort: expectedChartCohort, completed_views: evidence.chart_pages.filter(result => result.ok).map(chartKey), navigation_cohort: navigationWidths.flatMap(width => navigationPages.map(name => `${width}:${name}`)), navigation_views: navigationViews, static_cohort: staticPages, static_views: staticViews, phase_timings_ms: phaseTimings };
    const name = selection.shard.count === 1 && failure ? 'docs-failed.json' : 'docs' + suffix + '.json';
    const evidencePath = h.save(name, report);
    if (out) {
      mkdirSync(out, { recursive: true });
      copyFileSync(evidencePath, join(out, name));
    }
    console.log(JSON.stringify({ ...report, evidence_path: evidencePath }, null, 2));
  } finally {
    try { await navigationContext?.close(); } finally { await h.close(); }
  }
}
