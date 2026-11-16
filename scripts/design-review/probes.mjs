// Page-side instrumentation. `init` runs before any page script and defines
// `window.__review`; Node calls its methods through `probe(page, name, ...args)`.
// The function is serialized into the page, so it cannot use module scope.
//
// Charts are canvases drawn by the report runtime (`figure[data-rv-chart]`). The runtime emits
// no per-frame event, so everything here reads `window.__richview`: `charts()` is each chart's
// geometry model, `pending()` the ids still entering or gliding, and `settled()` finishes every
// entrance at once (so it is only called where a settled page is wanted).

export const probe = (page, name, ...args) => page.evaluate(([name, args]) => window.__review[name](...args), [name, args]);

export function init() {
  const review = window.__review = { shifts: [], mounts: {}, pointer: [] };
  const describe = node => {
    const e = node?.nodeType === 1 ? node : node?.parentElement;
    if (!e) return '';
    const classes = typeof e.className === 'string' ? e.className.trim().split(/\s+/).filter(Boolean).slice(0, 2) : [];
    return e.tagName.toLowerCase() + (e.id ? `#${e.id}` : '') + classes.map(c => `.${c}`).join('');
  };
  const frame = () => new Promise(accept => requestAnimationFrame(() => accept()));
  const sleep = ms => new Promise(accept => setTimeout(accept, ms));

  // Every shift is kept with its input flag. Touch emulation marks the shifts of a loading page
  // as following input although nobody touched it, so before the harness gives any input the
  // flag is ignored; `layout` counts only shifts that precede the first harness input.
  try {
    new PerformanceObserver(list => {
      for (const shift of list.getEntries()) review.shifts.push({ value: shift.value, at: shift.startTime, input: shift.hadRecentInput, sources: (shift.sources || []).map(s => describe(s.node)).filter(Boolean) });
    }).observe({ type: 'layout-shift', buffered: true });
  } catch { /* layout-shift entries are Chromium-only */ }
  for (const type of ['pointerdown', 'keydown']) addEventListener(type, event => { if (event.isTrusted) review.input ??= performance.now(); }, true);
  // Times of trusted pointer input, so a probe measures from the moment a reader's input arrived.
  for (const type of ['pointerdown', 'pointermove', 'touchstart']) addEventListener(type, event => { if (event.isTrusted) review.pointer.push(event.timeStamp); }, true);

  // Mount timing: the first frame in which a chart's canvas has a size, with the figure's height
  // then. Figures are mounted at load whether or not they are on screen.
  const watchMounts = () => {
    const figures = [...document.querySelectorAll('figure[data-rv-chart]')];
    for (const figure of figures) {
      const canvas = figure.querySelector('canvas.rv-chart__canvas');
      if (figure.id && canvas?.width > 0 && !review.mounts[figure.id]) review.mounts[figure.id] = { at: performance.now(), height: figure.getBoundingClientRect().height };
    }
    if (performance.now() < 30000 && (!figures.length || figures.some(figure => !review.mounts[figure.id]))) requestAnimationFrame(watchMounts);
  };
  addEventListener('DOMContentLoaded', () => requestAnimationFrame(watchMounts), { once: true });

  // CSS animations, transitions and Web Animations in the page (the report's own chrome) are
  // finished by settle; chart motion is the runtime's and is read from its canvas.
  const model = id => window.__richview?.charts().find(chart => chart.figure === id) ?? null;
  const canvasOf = id => document.getElementById(id)?.querySelector('canvas.rv-chart__canvas') ?? null;

  review.charts = () => {
    const models = window.__richview ? window.__richview.charts() : [];
    return [...document.querySelectorAll('figure[data-rv-chart]')].map((figure, index) => {
      const m = models.find(chart => chart.figure === figure.id);
      const geometry = m?.geometry;
      return {
        id: figure.id || `chart-${index + 1}`, type: geometry?.kind ?? 'unmodelled', title: figure.querySelector('.rv-chart__title')?.textContent.trim() ?? '',
        series: geometry?.series.length ?? 0, panels: geometry?.panels.length ?? 0, observations: geometry?.panels.reduce((n, p) => n + p.observations.length, 0) ?? 0,
        legend_entries: figure.querySelectorAll('.rv-chart__legend-entry[data-rv-series]').length, dropped_labels: geometry?.dropped.length ?? 0,
        entrance: geometry?.entrance ?? null, mount_ms: review.mounts[figure.id] ? +review.mounts[figure.id].at.toFixed(1) : null,
      };
    });
  };
  review.figure = id => document.getElementById(id);
  review.center = id => review.figure(id)?.scrollIntoView({ block: 'center', behavior: 'instant' });
  // Crop of a chart figure with a margin for hover shadows and pops: in viewport
  // coordinates when it is in view, otherwise in document coordinates.
  review.clip = (id, margin = 12) => {
    const r = review.figure(id)?.getBoundingClientRect();
    if (!r) return null;
    let left = Math.floor(r.left - margin), top = Math.floor(r.top - margin), right = Math.ceil(r.right + margin), bottom = Math.ceil(r.bottom + margin);
    const inside = left >= 0 && top >= 0 && right <= innerWidth && bottom <= innerHeight;
    if (inside) return { x: left, y: top, width: right - left, height: bottom - top, page: false };
    const x = Math.max(0, left + scrollX), y = Math.max(0, top + scrollY);
    return { x, y, width: right + scrollX - x, height: bottom + scrollY - y, page: true };
  };
  // Ids of the charts that are still entering, gliding or waiting for their entrance.
  review.pending = () => window.__richview?.pending() ?? [];

  // Real-clock settle: the runtime's `settled()` (every chart drawn at rest, fonts loaded),
  // then finite page animations. Returns whether the runtime started and how long
  // `settled()` took to resolve, in milliseconds since navigation began.
  review.settle = async (timeout = 30000) => {
    const end = performance.now() + timeout;
    const charts = () => document.querySelector('figure[data-rv-chart]');
    while (!window.__richview && charts() && performance.now() < end) await frame();
    let ok = !charts() || !!window.__richview;
    if (window.__richview) {
      try { await window.__richview.settled(); } catch { ok = false; }
    }
    review.settledAt = performance.now();
    const finite = document.getAnimations().filter(a => Number.isFinite(a.effect?.getComputedTiming().endTime));
    await Promise.race([Promise.allSettled(finite.map(a => a.finished)), sleep(Math.max(0, end - performance.now()))]);
    await frame(); await frame();
    return { ok: ok && document.fonts.status === 'loaded', charts: document.querySelectorAll('figure[data-rv-chart]').length, settled_ms: +review.settledAt.toFixed(1) };
  };
  // Scroll through the page so content revealed on entering the viewport is shown.
  review.reveal = async () => {
    for (let y = 0; y < document.documentElement.scrollHeight; y += innerHeight) { scrollTo({ top: y, behavior: 'instant' }); await frame(); }
    scrollTo({ top: 0, behavior: 'instant' }); await frame();
  };

  // Chart text from the geometry model. Each text item carries its box, so a label that leaves
  // the chart, or two that cover each other, are found without the DOM. The model's own
  // checks (clipping and label collisions it already found) are added; labels it dropped on
  // purpose are counted in `charts`, not reported.
  review.glyphs = () => {
    const models = window.__richview ? window.__richview.charts() : [];
    return Object.fromEntries(models.map(m => {
      const g = m.geometry, found = [];
      // Dropped and thinned labels are the runtime's deliberate choice, not findings.
      for (const check of g.checks ?? []) if (check.severity !== 'info' && !/^label-(dropped|thinned)$/.test(check.kind)) found.push({ kind: check.kind, source: 'model', message: check.message });
      const texts = g.panels.flatMap(panel => panel.items.filter(item => item.kind === 'text' && item.bounds && item.text?.trim()));
      for (const t of texts) {
        const b = t.bounds;
        if (b.x < -.5 || b.y < -.5 || b.x + b.w > g.width + .5 || b.y + b.h > g.height + .5) found.push({ kind: 'clipping', source: 'bounds', message: `${t.role} label '${t.text}' exceeds the chart`, region: b });
      }
      texts.sort((a, b) => a.bounds.y - b.bounds.y);
      for (let i = 0; i < texts.length; i++) {
        const a = texts[i].bounds;
        for (let j = i + 1; j < texts.length && texts[j].bounds.y < a.y + a.h; j++) {
          const b = texts[j].bounds;
          const x = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x), y = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
          if (x > 1 && y > Math.max(1, .2 * Math.min(a.h, b.h))) found.push({ kind: 'label-collision', source: 'bounds', message: `'${texts[i].text}' overlaps '${texts[j].text}'`, region: a });
        }
      }
      return [m.figure, found];
    }));
  };

  // Report text: collisions between text of different blocks, text cut by a clipping
  // container or the viewport, text out of view in a scroll container, horizontal page
  // overflow and layout shift.
  review.layout = () => {
    const width = document.documentElement.clientWidth;
    const styles = new Map(), clippers = new Map();
    const style = e => styles.get(e) ?? styles.set(e, getComputedStyle(e)).get(e);
    const blockOf = e => { while (e.parentElement && ['inline', 'contents'].includes(style(e).display)) e = e.parentElement; return e; };
    // The nearest element, from `e` itself upwards, whose overflow clips its content.
    const clipperOf = e => {
      if (!clippers.has(e)) {
        let found = null;
        for (let p = e; p && p !== document.body && p !== document.documentElement; p = p.parentElement) {
          if (style(p).overflowX !== 'visible' || style(p).overflowY !== 'visible') { found = p; break; }
        }
        clippers.set(e, found);
      }
      return clippers.get(e);
    };
    // Rendered, and not visually hidden accessible text (a box of one or two pixels).
    const shown = e => {
      if (!e.checkVisibility({ opacityProperty: true, visibilityProperty: true })) return false;
      const r = e.getBoundingClientRect(), c = clipperOf(e);
      return (style(e).display === 'contents' || (r.width > 2 && r.height > 2)) && !(c && (c.clientWidth <= 2 || c.clientHeight <= 2));
    };
    const floating = e => { for (let p = e; p && p !== document.documentElement; p = p.parentElement) if (['fixed', 'sticky'].includes(style(p).position)) return true; return false; };
    const boxes = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const e = node.parentElement;
      if (!node.data.trim() || !e || e.closest('script,style,noscript,template,svg') || !shown(e)) continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      for (const r of range.getClientRects()) if (r.width > 1 && r.height > 1) boxes.push({ text: node.data.trim(), e, block: blockOf(e), r });
    }
    boxes.sort((a, b) => a.r.top - b.r.top);
    const overlaps = [], pairs = new Set(), ids = new Map();
    const id = e => ids.get(e) ?? ids.set(e, ids.size).get(e);
    for (let i = 0; i < boxes.length; i++) {
      const a = boxes[i];
      for (let j = i + 1; j < boxes.length && boxes[j].r.top < a.r.bottom; j++) {
        const b = boxes[j];
        // Lines of one block may touch through tight leading. Chart canvas text is not DOM text;
        // `glyphs` measures it from the geometry model.
        if (a.block === b.block) continue;
        const x = Math.min(a.r.right, b.r.right) - Math.max(a.r.left, b.r.left);
        const y = Math.min(a.r.bottom, b.r.bottom) - Math.max(a.r.top, b.r.top);
        const key = `${id(a.e)}:${id(b.e)}`;
        if (x > 1 && y > Math.max(1, .2 * Math.min(a.r.height, b.r.height)) && !pairs.has(key) && !floating(a.e) && !floating(b.e)) {
          pairs.add(key);
          overlaps.push({ a: a.text.slice(0, 80), b: b.text.slice(0, 80), elements: [describe(a.e), describe(b.e)], overlap: [Math.round(x), Math.round(y)] });
        }
      }
    }
    // Text outside its clipping box is cut off; inside a scroll container it is out of view.
    // A line's box includes the font's full ascent and descent, which tight leading lets
    // extend past the container without cutting glyphs, so vertically a quarter line counts.
    const clipping = [], scrolled = [], seen = new Set();
    for (const box of boxes) {
      if (seen.has(box.e)) continue;
      const c = clipperOf(box.e), slack = box.r.height / 4;
      let outside;
      if (c) {
        const r = c.getBoundingClientRect(), left = r.left + c.clientLeft, top = r.top + c.clientTop;
        outside = box.r.left < left - 1 || box.r.right > left + c.clientWidth + 1 || box.r.top < top - slack || box.r.bottom > top + c.clientHeight + slack;
      } else outside = box.r.left < -1 || box.r.right > width + 1;
      if (!outside) continue;
      seen.add(box.e);
      const s = c && style(c), scrolls = !!s && /auto|scroll/.test(s.overflowX + s.overflowY);
      (scrolls ? scrolled : clipping).push({ text: box.text.slice(0, 80), element: describe(box.e), container: c ? describe(c) : 'viewport', ellipsis: style(box.e).textOverflow === 'ellipsis' });
    }
    const overflow = Math.max(0, document.documentElement.scrollWidth - width);
    const offenders = [];
    if (overflow > .5) {
      const wide = [...document.body.querySelectorAll('*')].filter(e => { const r = e.getBoundingClientRect(); return r.width > 0 && r.right > width + .5 && !(e.parentElement && clipperOf(e.parentElement)); });
      for (const e of wide) if (!wide.some(other => other !== e && e.contains(other))) offenders.push({ element: describe(e), right: Math.round(e.getBoundingClientRect().right) });
    }
    let cls = 0, session = 0, first = -Infinity, last = -Infinity;
    for (const shift of review.shifts.filter(s => review.input === undefined || s.at < review.input)) {
      if (shift.at - last > 1000 || shift.at - first > 5000) { session = 0; first = shift.at; }
      session += shift.value; last = shift.at; cls = Math.max(cls, session);
    }
    const sources = [...new Set(review.shifts.toSorted((a, b) => b.value - a.value).flatMap(s => s.sources))].slice(0, 5);
    return {
      text: { boxes: boxes.length, overlaps: overlaps.length, overlap_examples: overlaps.slice(0, 20), clipping: clipping.length, clipping_examples: clipping.slice(0, 20), scroll_hidden: scrolled.length, scroll_hidden_examples: scrolled.slice(0, 10) },
      overflow: { px: Math.round(overflow), offenders: offenders.slice(0, 8) },
      cls: { value: +cls.toFixed(4), shifts: review.shifts.length, sources },
      page: { width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight },
    };
  };


  // A pointer target on a representative mark, in viewport coordinates, from the geometry
  // model: the largest bar or slice, the middle point or cell, or the observation of a line or
  // area nearest to 62% of its length.
  review.target = id => {
    const m = model(id), canvas = canvasOf(id);
    const panel = m?.geometry.panels[0];
    if (!panel?.observations.length || !canvas) return null;
    const kind = m.geometry.kind, list = panel.observations;
    const largest = size => list.reduce((a, b) => (size(b) > size(a) ? b : a));
    let pick, mark;
    if (kind === 'bar') { pick = largest(o => (o.area ? o.area.w * o.area.h : 0)); mark = 'bar'; }
    else if (kind === 'pie') { pick = largest(o => (o.wedge ? o.wedge.end - o.wedge.start : 0)); mark = 'slice'; }
    else if (kind === 'heatmap') { pick = list[list.length >> 1]; mark = 'cell'; }
    else if (kind === 'scatter') { pick = list[list.length >> 1]; mark = 'point'; }
    else { pick = list[Math.round((list.length - 1) * .62)]; mark = kind; }
    const at = pick.area ? [pick.area.x + pick.area.w / 2, pick.area.y + pick.area.h / 2] : pick.dots?.[0]?.at ?? pick.at;
    const r = canvas.getBoundingClientRect();
    return { x: r.left + at[0], y: r.top + at[1], mark, label: pick.readout?.heading ?? '', readout: (pick.readout?.lines ?? []).map(l => `${l.name} ${l.value}`).join(', ') };
  };

  // The chart readout is shown when it is displayed and mostly opaque; returns its opacity or 0.
  review.readout = id => {
    const tip = review.figure(id)?.querySelector('.rv-chart__readout');
    if (!tip || tip.hidden) return 0;
    const style = getComputedStyle(tip);
    return style.display === 'none' || style.visibility === 'hidden' ? 0 : +parseFloat(style.opacity).toFixed(2);
  };

  // The canvas as it is drawn now, as PNG base64.
  const encode = async canvas => {
    const blob = await canvas.convertToBlob({ type: 'image/png' });
    const bytes = new Uint8Array(await blob.arrayBuffer());
    let text = '';
    for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return btoa(text);
  };
  const copy = canvas => {
    const out = new OffscreenCanvas(canvas.width, canvas.height);
    out.getContext('2d').drawImage(canvas, 0, 0);
    return out;
  };
  review.canvasPng = async id => { const canvas = canvasOf(id); return canvas ? encode(copy(canvas)) : null; };

  // Frames of a chart canvas on the page's own frame timer. The runtime emits no per-frame
  // event, so each animation frame is read from the canvas: for every time in `targets` (ms)
  // the first frame at or after it is kept with its actual time. Time 0 is now (`from` is
  // 'now') or the first pointer input after this call ('input'). A frame may be one frame
  // older than the input it follows, when the page draws after this callback runs.
  review.film = (id, targets, from, timeout = 4000) => new Promise(done => {
    const figure = review.figure(id), canvas = canvasOf(id);
    if (!canvas) { done([]); return; }
    const armed = performance.now(), kept = [];
    let t0 = from === 'now' ? armed : null, next = 0, last = null;
    const readout = () => {
      const tip = figure.querySelector('.rv-chart__readout');
      return tip && !tip.hidden && getComputedStyle(tip).display !== 'none' ? tip.innerText.replace(/\s+/g, ' ').trim() : null;
    };
    const finish = async () => {
      const out = [];
      for (const k of kept) out.push({ target: k.target, t: +k.t.toFixed(1), readout: k.readout, png: k.encoded ??= await encode(k.canvas) });
      done(out);
    };
    const loop = ts => {
      if (t0 === null) t0 = review.pointer.find(at => at >= armed) ?? null;
      if (t0 === null && ts - armed > timeout) { finish(); return; }
      if (t0 !== null && ts >= t0) {
        const e = ts - t0;
        if (next < targets.length && e >= targets[next]) {
          last = { t: e, canvas: copy(canvas), readout: readout() };
          while (next < targets.length && e >= targets[next]) kept.push({ ...last, target: targets[next++] });
        }
        if (next >= targets.length) { finish(); return; }
        if (e > targets.at(-1) + 600) { finish(); return; }
      }
      requestAnimationFrame(loop);
    };
    requestAnimationFrame(loop);
  });

  // The whole canvas kept as a reference, and the share of its pixels that differ now.
  review.snapshot = id => { const c = canvasOf(id); review.snap = c.getContext('2d').getImageData(0, 0, c.width, c.height).data; };
  review.differs = id => {
    const c = canvasOf(id), now = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
    let n = 0;
    for (let i = 0; i < now.length; i += 4) if (Math.abs(now[i] - review.snap[i]) + Math.abs(now[i + 1] - review.snap[i + 1]) + Math.abs(now[i + 2] - review.snap[i + 2]) > 24) n++;
    return +(n / (now.length / 4)).toFixed(5);
  };
}
