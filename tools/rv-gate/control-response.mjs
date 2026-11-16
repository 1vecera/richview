// Host reason (ARC-03): Playwright and a loopback HTTP server measure real report controls.
// FEA-BR-36 / PRF-20: event handler cost (Rust layout, WebGL updates and DOM writes) and
// input to two animation frames, on the two golden inputs at desktop and phone 4x CPU.
// The caller owns the quiet-host / benchmark hold. These are control-response measurements;
// animated-frame script and pacing remain the separate PRF-21 / PRF-27 probes.
import { launchBrowser } from "./lib/browser-launch.mjs";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { quantile } from "./lib/perf.mjs";

export const CONTROL_PROFILES = [
  { id: "desktop", width: 1440, height: 1000, cpu: 1 },
  { id: "phone4x", width: 390, height: 844, cpu: 4 },
];
export const CONTROL_INPUTS = [
  { id: "gapminder", source: "playable-gapminder", actions: ["button", "step", "scrubber"] },
  { id: "indicator", source: "playable-line-indicator", actions: ["button", "step", "scrubber"] },
  { id: "indicator-dropdown", source: "playable-line-indicator", actions: ["dropdown", "step", "scrubber"], dropdown: true },
];
export const CONTROL_SAMPLING = { samples: 10, warmup: 1 };
const { samples: SAMPLES, warmup: WARMUP } = CONTROL_SAMPLING;
const skip = new Set(["Rendered", "research", "node_modules", ".git"]);
const key = (profile, input, action, measure) => `controls.${profile}.${input}.${action}.${measure}.p95`;
export const CONTROL_RESPONSE_BUDGETS = Object.fromEntries(CONTROL_PROFILES.flatMap(profile =>
  CONTROL_INPUTS.flatMap(input => input.actions.flatMap(action => [
    [key(profile.id, input.id, action, "layout"), { limit: 100, unit: "ms", why: "FEA-BR-36 / PRF-20", timing: true }],
    [key(profile.id, input.id, action, "two-frames"), { limit: 100, unit: "ms", why: "FEA-BR-36 / PRF-20", timing: true }],
  ]))));

function stage(cwd, bin, out, input) {
  const folder = join(out, "inputs", `controls-${input.id}`);
  rmSync(folder, { recursive: true, force: true });
  cpSync(join(cwd, "examples", input.source), folder, { recursive: true, filter: path => !skip.has(basename(path)) });
  const source = join(folder, "report.rmd");
  let text = readFileSync(source, "utf8");
  if (input.dropdown) {
    if (!text.includes("as: buttons")) throw new Error(`${input.id}: the golden input no longer has a button selector`);
    text = text.replace("as: buttons", "as: dropdown");
    writeFileSync(source, text);
  }
  const result = spawnSync(bin, ["generate", source, "-o", "controls.html", "--format", "json"], { cwd, encoding: "utf8" });
  if (result.status !== 0) throw new Error(`${input.id}: generate failed: ${result.stderr || result.stdout}`);
  const generated = JSON.parse(result.stdout);
  if (!generated.ok || generated.diagnostics?.some(d => d.level === "error")) throw new Error(`${input.id}: golden input did not render cleanly`);
  return { html: readFileSync(generated.output), sourceHash: createHash("sha256").update(text).update(readFileSync(join(folder, "data", input.id === "gapminder" ? "gapminder.csv" : "indicator.csv"))).digest("hex") };
}

/** Measure one real input and verify the runtime accepted it outside both timed regions. */
async function sample(page, action) {
  return page.evaluate(async action => {
    const figure = document.querySelector("figure[data-rv-chart]");
    const before = window.__richview.charts()[0].state;
    let invoke;
    if (action === "button") {
      const button = figure.querySelector('.rv-chart__option[aria-pressed="false"]');
      if (!button) throw new Error("button action has no unselected option");
      invoke = () => button.click();
    } else if (action === "dropdown") {
      const select = figure.querySelector(".rv-chart__select");
      if (!select || select.options.length < 2) throw new Error("dropdown action has fewer than two options");
      const chosen = String((select.selectedIndex + 1) % select.options.length);
      invoke = () => { select.value = chosen; select.dispatchEvent(new Event("change", { bubbles: true })); };
    } else {
      const scrubber = figure.querySelector(".rv-chart__scrubber");
      if (!scrubber || Number(scrubber.max) < 1) throw new Error("playback action has fewer than two frames");
      const current = Number(scrubber.value), max = Number(scrubber.max);
      if (action === "scrubber") {
        invoke = () => { scrubber.value = String(current === 0 ? max : 0); scrubber.dispatchEvent(new Event("input", { bubbles: true })); };
      } else {
        const direction = current === max ? "previous" : "next";
        const button = figure.querySelector(`.rv-chart__control-button--${direction}`);
        if (!button) throw new Error(`step action has no ${direction} button`);
        invoke = () => button.click();
      }
    }
    const started = performance.now();
    invoke();
    const layout = performance.now() - started;
    await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
    const twoFrames = performance.now() - started;
    const after = window.__richview.charts()[0].state;
    if (before === after) throw new Error(`${action} did not change the runtime state`);
    await window.__richview.settled();
    return { layout, twoFrames };
  }, action);
}

/** Daniel29Oct: ten measured changes after one warmup for every input, control and profile. */
export async function measureControlResponse({ cwd, bin, out }) {
  cwd = resolve(cwd); bin = resolve(bin); out = resolve(out);
  mkdirSync(out, { recursive: true });
  const inputs = new Map(CONTROL_INPUTS.map(input => [input.id, stage(cwd, bin, out, input)]));
  const server = createServer((request, response) => {
    const input = inputs.get(new URL(request.url, "http://localhost").pathname.slice(1).replace(/\.html$/, ""));
    if (!input) return void response.writeHead(404).end();
    response.writeHead(200, { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" }).end(input.html);
  });
  await new Promise((done, reject) => { server.once("error", reject); server.listen(0, "127.0.0.1", done); });
  let browser;
  const records = [], values = {};
  try {
    const { chromium, errors: playwrightErrors } = await import(pathToFileURL(join(cwd, "node_modules/playwright/index.mjs")).href);
    browser = await launchBrowser(chromium, { headless: true, ...(process.env.RICHVIEW_CHROMIUM ? { executablePath: process.env.RICHVIEW_CHROMIUM } : {}) }, { TimeoutError: playwrightErrors.TimeoutError });
    for (const profile of CONTROL_PROFILES) {
      const context = await browser.newContext({ viewport: { width: profile.width, height: profile.height }, deviceScaleFactor: 1, hasTouch: profile.cpu > 1, reducedMotion: "reduce" });
      try {
        const page = await context.newPage();
        const errors = [];
        page.on("pageerror", error => errors.push(error.message));
        const cdp = await context.newCDPSession(page);
        await cdp.send("Emulation.setCPUThrottlingRate", { rate: profile.cpu });
        for (const input of CONTROL_INPUTS) {
          await page.goto(`http://127.0.0.1:${server.address().port}/${input.id}.html`);
          await page.waitForFunction(() => window.__richview?.charts().length === 1, null, { timeout: 45_000 });
          await page.evaluate(() => document.fonts.ready);
          await page.evaluate(() => window.__richview.settled());
          const setup = await page.evaluate(() => {
            const figure = document.querySelector("figure[data-rv-chart]");
            const payload = JSON.parse(figure.querySelector("script[data-rv-payload]").textContent);
            const canvas = document.createElement("canvas");
            const gl = canvas.getContext("webgl2");
            if (!gl) throw new Error("WebGL2 renderer information is unavailable");
            const info = gl.getExtension("WEBGL_debug_renderer_info");
            const gpu = { vendor: gl.getParameter(info ? info.UNMASKED_VENDOR_WEBGL : gl.VENDOR), renderer: gl.getParameter(info ? info.UNMASKED_RENDERER_WEBGL : gl.RENDERER) };
            gl.getExtension("WEBGL_lose_context")?.loseContext();
            return { rows: payload.rows.length, gpu };
          });
          if (setup.rows > 1000 || setup.rows < 1) throw new Error(`${input.id}: ${setup.rows} rows falls outside FEA-BR-36's 1..1000-row scope`);
          for (const action of input.actions) {
            for (let i = 0; i < WARMUP; i++) await sample(page, action);
            const samples = [];
            for (let i = 0; i < SAMPLES; i++) samples.push(await sample(page, action));
            if (samples.some(s => !Number.isFinite(s.layout) || !Number.isFinite(s.twoFrames))) throw new Error("control timing contains a non-finite sample");
            values[key(profile.id, input.id, action, "layout")] = quantile(samples.map(s => s.layout), 0.95);
            values[key(profile.id, input.id, action, "two-frames")] = quantile(samples.map(s => s.twoFrames), 0.95);
            records.push({ profile: profile.id, cpu: profile.cpu, input: input.id, sourceHash: inputs.get(input.id).sourceHash, action, ...setup, samples });
          }
          if (errors.length) throw new Error(`${input.id}: browser errors: ${errors.join("; ")}`);
        }
      } finally { await context.close(); }
    }
    const result = { browser: `chromium ${browser.version()}`, samplesPerCase: SAMPLES, warmupPerCase: WARMUP, unit: "ms", values, records };
    writeFileSync(join(out, "control-response.json"), `${JSON.stringify(result, null, 2)}\n`);
    return result;
  } finally {
    await browser?.close();
    await new Promise(done => server.close(done));
  }
}
