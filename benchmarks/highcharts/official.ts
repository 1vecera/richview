// Read/capture actual official sample configurations. The CDN is blocked on some
// headless networks; fulfill its script requests with the pinned local reference.
import { chromium } from 'playwright';
import { mkdirSync, writeFileSync, existsSync } from 'node:fs';
import { resolve, join } from 'node:path';

interface SampleChart {
  title?: { textStr: string };
  series: { type: string; points: unknown[] }[];
  axes: { min?: number; max?: number; tickPositions?: number[] }[];
  tooltip: { shared?: boolean };
  options: { chart: { zooming?: unknown } };
}

// The official sample's own Highcharts script defines this global inside the page.
declare const Highcharts: { version: string; charts: (SampleChart | undefined)[] };
const reference = resolve(process.argv[2], 'reference');
const output = join(reference, 'live'); mkdirSync(output, { recursive: true });
const browser = await chromium.launch({ headless: true, executablePath: process.env.RICHVIEW_CHROMIUM || '/usr/bin/chromium' });
const records = [];
try {
  for (const slug of ['line-time-series', 'column-basic', 'area-missing', 'heatmap', 'spline-irregular-time']) {
    const page = await browser.newPage({ viewport: { width: 720, height: 600 } });
    await page.route('https://code.highcharts.com/**', async route => {
      const file = join(reference, 'node_modules/highcharts', new URL(route.request().url()).pathname);
      if (!existsSync(file)) throw new Error(`Missing local module ${file}`);
      await route.fulfill({ path: file, contentType: 'text/javascript' });
    });
    const url = `https://www.highcharts.com/samples/nonav/highcharts/demo/${slug}`;
    await page.goto(url, { waitUntil: 'domcontentloaded' });
    await page.locator('.highcharts-container').first().waitFor({ timeout: 30000 });
    await page.waitForTimeout(1200);
    const record = await page.evaluate(() => ({ version: Highcharts.version, charts: Highcharts.charts.filter((c): c is SampleChart => Boolean(c)).map(c => ({ title: c.title?.textStr, types: c.series.map(s => s.type), points: c.series.map(s => s.points.length), axes: c.axes.map(a => ({ min: a.min, max: a.max, ticks: a.tickPositions })), shared_tooltip: c.tooltip.shared, zooming: c.options.chart.zooming })) }));
    await page.locator('.highcharts-container').first().screenshot({ path: join(output, `${slug}-rest.png`) });
    const box = await page.locator('.highcharts-container').first().boundingBox();
    if (!box) throw new Error(`The Highcharts example ${slug} has no visible chart box`);
    await page.mouse.move(box.x + box.width * .6, box.y + box.height * .5); await page.waitForTimeout(120);
    await page.locator('.highcharts-container').first().screenshot({ path: join(output, `${slug}-hover.png`) });
    records.push({ url, demo_url: `https://www.highcharts.com/demo/highcharts/${slug}`, captured_at: new Date().toISOString(), provenance: 'Actual official sample HTML/data, headless Chromium; CDN scripts fulfilled by pinned local Highcharts installation because upstream CDN was blocked by ORB', viewport: { width: 720, height: 600 }, ...record });
    await page.close();
  }
} finally { writeFileSync(join(output, 'index.json'), JSON.stringify(records, null, 2) + '\n'); await browser.close(); }
console.log(`${records.length} live official examples captured`);
