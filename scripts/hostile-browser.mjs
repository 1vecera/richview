// Host reason (ARC-03): observe actual hostile-report DOM/runtime effects and stop browser egress.
import { performance } from 'node:perf_hooks';

function installAudit({ canaries, forbidden_ids = [], forbidden_classes = [], tab_ids = [] }) {
  const send = globalThis.__rvHostileRecord;
  const pending = [];
  const seen = new Set();
  const record = (kind, detail) => {
    const key = JSON.stringify([kind, detail]);
    if (seen.has(key)) return;
    seen.add(key);
    pending.push(send({ kind, ...detail }));
  };
  for (const name of canaries) Object.defineProperty(globalThis, name, {
    configurable: false, writable: false,
    value: (...args) => record('script', { name, args: args.map(value => ['string', 'number', 'boolean'].includes(typeof value) ? value : typeof value) }),
  });
  document.addEventListener('securitypolicyviolation', event => record('csp', {
    directive: event.effectiveDirective, blocked: event.blockedURI, disposition: event.disposition,
  }), true);
  const badStyle = text => /HOSTILE|@import|expression\s*\(|javascript\s*:|-moz-binding/i.test(text)
    || [...text.matchAll(/url\(([^)]*)\)/gi)].some(match => !/^(?:data:|blob:|#)/i.test(match[1].trim().replace(/^["']/, '')));
  const inspect = node => {
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const tag = node.localName;
    if (/HOSTILE/i.test(tag)) record('markup', { tag });
    // The runtime creates these declared tab buttons after the native heading-id check.
    const tab = node.getAttribute('data-rv-tab');
    const generatedTab = tag === 'button' && tab_ids.includes(tab) && node.id === `tab-${tab}`
      && node.getAttribute('role') === 'tab' && node.classList.contains('rv-tabs__tab');
    if (forbidden_ids.includes(node.id) && !generatedTab || /^rv-hostile/i.test(node.id)) record('markup', { tag, id: node.id });
    for (const name of node.classList) if (forbidden_classes.includes(name) || /^rv-hostile/i.test(name)) record('markup', { tag, class: name });
    for (const { name, value } of node.attributes) {
      if (/^on/i.test(name) || name === 'srcdoc') record('markup', { tag, attribute: name, value: value.slice(0, 160) });
      if (name === 'style' && badStyle(value)) record('style', { tag, value: value.slice(0, 160) });
      const uri = value.replace(/[\s\u0000-\u001f]/g, '');
      // SVG loaded by an img is an image resource, not an executable SVG document.
      const svgImage = tag === 'img' && name === 'src' && /^data:image\/svg\+xml;base64,/i.test(uri);
      if (['href', 'xlink:href', 'src', 'action', 'formaction'].includes(name) && !svgImage
          && /^(?:javascript:|vbscript:|file:|data:(?:text\/(?:html|javascript)|image\/svg))/i.test(uri)) {
        record('markup', { tag, attribute: name, value: value.slice(0, 160) });
      }
    }
    if (tag === 'script' && node.type !== 'application/json' && /HOSTILE/.test(node.textContent)) record('markup', { tag, executable: true });
    if (tag === 'style' && badStyle(node.textContent)) record('style', { tag, value: node.textContent.slice(0, 160) });
  };
  const scan = root => {
    inspect(root);
    for (const node of root.querySelectorAll?.('*') ?? []) inspect(node);
  };
  new MutationObserver(records => {
    for (const mutation of records) {
      if (mutation.type === 'attributes') inspect(mutation.target);
      else for (const node of mutation.addedNodes) scan(node);
    }
  }).observe(document, { subtree: true, childList: true, attributes: true });
  if (navigator.serviceWorker) {
    navigator.serviceWorker.register = (...args) => {
      record('network', { transport: 'service-worker', url: String(args[0]) });
      return Promise.reject(new Error('Hostile audit blocks service-worker registration'));
    };
  }
  Object.defineProperty(globalThis, '__rvHostileScan', { configurable: false, writable: false, value: async () => {
    scan(document);
    await Promise.all(pending);
  } });
}

async function within(promise, deadline, label) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label}: hostile browser deadline exceeded`)), Math.max(1, deadline - performance.now()));
    })]);
  } finally { clearTimeout(timer); }
}

/** Audit one real report in its own context, including its runtime, tabs, readouts and Tables. */
export async function auditHostileReport(browser, { url, contract, canaries, deadline }) {
  const findings = [];
  const checkpoints = [];
  const context = await browser.newContext({ viewport: { width: 1100, height: 800 }, reducedMotion: 'reduce', serviceWorkers: 'block' });
  context.setDefaultTimeout(Math.max(1, Math.min(5000, deadline - performance.now())));
  let page;
  let documentRequested = false;
  try {
    await context.exposeBinding('__rvHostileRecord', (_, finding) => { findings.push(finding); });
    await context.addInitScript(installAudit, { ...contract, canaries });
    await context.route('**/*', async route => {
      const request = route.request();
      if (/^(?:data|blob):/.test(request.url())) await route.continue();
      else if (!documentRequested && request.url() === url && request.isNavigationRequest() && request.frame() === page.mainFrame()) {
        documentRequested = true;
        await route.continue();
      }
      else {
        findings.push({ kind: 'network', transport: request.resourceType(), url: request.url() });
        await route.abort('blockedbyclient');
      }
    });
    await context.routeWebSocket('**/*', route => {
      findings.push({ kind: 'network', transport: 'websocket', url: route.url() });
      route.close();
    });
    page = await context.newPage();
    page.on('pageerror', error => findings.push({ kind: 'pageerror', message: error.message }));
    page.on('dialog', async dialog => {
      findings.push({ kind: 'script', dialog: dialog.type(), message: dialog.message() });
      await dialog.dismiss();
    });
    context.on('page', other => { if (other !== page) findings.push({ kind: 'markup', popup: true }); });
    const response = await page.goto(url, { waitUntil: 'load', timeout: Math.max(1, Math.min(5000, deadline - performance.now())) });
    if (!response || response.status() !== 200) throw new Error(`Report HTTP ${response?.status() ?? 'missing'}`);
    const initial = await page.evaluate(() => ({
      charts: document.querySelectorAll('figure[data-rv-chart]').length,
      tabs: document.querySelectorAll('.rv-tabpanel[data-rv-tab]').length,
      needsRuntime: !!document.querySelector('[data-rv-payload], [data-rv-tab], [data-rv-scrolly], [data-rv-embed]')
        || [...document.scripts].some(script => script.type !== 'application/json' && script.textContent.includes('richview_runtime')),
    }));
    if (initial.needsRuntime) {
      await page.waitForFunction(() => typeof window.__richview?.settled === 'function', null, { timeout: Math.max(1, Math.min(5000, deadline - performance.now())) });
      await within(page.evaluate(() => window.__richview.settled()), deadline, 'runtime settle');
    }
    const models = await page.evaluate(() => window.__richview?.charts?.() ?? []);
    if (models.length !== initial.charts) throw new Error(`Runtime has ${models.length} charts for ${initial.charts} figures`);
    if (contract.browser_charts !== undefined && initial.charts !== contract.browser_charts) throw new Error(`Expected ${contract.browser_charts} corpus charts, got ${initial.charts}`);
    const scan = async label => {
      await within(page.evaluate(async () => {
        await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        await window.__rvHostileScan();
      }), deadline, label);
      checkpoints.push(label);
    };
    await scan('hydrated');
    let sorts = 0;
    const headers = page.locator('table thead button:visible');
    if (await headers.count()) {
      for (let index = 0; index < 2; index++) {
        await headers.first().click();
        sorts++;
        await scan(`sort:${index}`);
      }
    }
    for (const tab of contract.tab_ids ?? []) {
      const button = page.locator('.rv-tabs__tab[data-rv-tab]');
      const tabs = await button.evaluateAll(nodes => nodes.map(node => ({ id: node.getAttribute('data-rv-tab'), text: node.textContent })));
      const at = tabs.findIndex(item => item.id === tab);
      if (at < 0) throw new Error(`Missing tab ${tab}`);
      await button.nth(at).click();
      await within(page.evaluate(() => window.__richview.settled()), deadline, 'tab settle');
      const selected = await button.nth(at).getAttribute('aria-selected');
      if (selected !== 'true') throw new Error(`Tab ${tab} did not select`);
      await scan(`tab:${tab}`);
    }
    const figures = page.locator('figure[data-rv-chart]');
    let readouts = 0, tables = 0;
    for (let index = 0; index < initial.charts; index++) {
      const figure = figures.nth(index);
      const frame = figure.locator('.rv-chart__frame');
      if (await figure.locator('canvas').count() === 0) throw new Error(`Chart ${index} did not hydrate`);
      if (models[index].geometry.panels.some(panel => panel.observations.length)) {
        await frame.focus();
        for (const key of ['Home', 'End']) {
          await page.keyboard.press(key);
          await within(page.evaluate(() => window.__richview.settled()), deadline, 'readout settle');
          const text = await figure.locator('.rv-chart__readout').evaluate(node => node.hidden ? '' : node.textContent.trim());
          if (!text) throw new Error(`Chart ${index} ${key} readout is empty`);
          readouts++;
          await scan(`chart:${index}:${key}`);
        }
      }
      await figure.locator('.rv-chart__action--table').click();
      if (!await figure.locator('.rv-chart__table table').isVisible()) throw new Error(`Chart ${index} Table did not open`);
      tables++;
      await scan(`chart:${index}:table`);
      await figure.locator('.rv-chart__action--table').click();
    }
    await scan('final');
    return { findings, checkpoints, charts: initial.charts, tabs: initial.tabs, readouts, tables, sorts };
  } catch (error) {
    findings.push({ kind: 'runtime', message: error.message.split('\n')[0] });
    return { findings, checkpoints };
  } finally { await context.close(); }
}
