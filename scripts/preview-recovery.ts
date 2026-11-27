// Host reason (ARC-03): bound recovery of lost browser preview documents.
import type { Frame, Page } from 'playwright';

// Read only after the original wait failed. Preserve partial evidence if a child world
// never responds; avoid layout reads that could themselves block a stalled document.
export const capturePreviewFailure = async (page: Page, retained: Frame, expected: string, error: unknown) => {
  const diagnostic: Record<string, unknown> = { expected, error: String(error) };
  const keep = (key: string, promise: Promise<unknown>) => promise.then(value => { diagnostic[key] = value; },
    reason => { diagnostic[key] = { error: String(reason) }; });
  const peek = (frame: Frame) => frame.evaluate(() => ({
    url: location.href, ready: document.readyState, title: document.title,
    runtime: document.documentElement.dataset.rvRuntime,
    body: document.body?.textContent?.slice(0, 1500),
    titles: [...document.querySelectorAll('.rv-chart__title')].map(node => node.textContent),
  }));
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      Promise.all([
        keep('parent', page.evaluate(expected => {
          const source = document.querySelector<HTMLTextAreaElement>('#source')!;
          const preview = document.querySelector<HTMLIFrameElement>('#preview')!;
          return {
            source: source.value.slice(0, 1200),
            sourceWindowBytes: new TextEncoder().encode(source.value).length,
            sourceLines: source.dataset.sourceLines,
            status: document.querySelector('#diagnostics')!.textContent,
            srcdocBytes: new TextEncoder().encode(preview.srcdoc).length,
            hasExpected: preview.srcdoc.includes(expected),
            sourceRendering: window.__rvEvidence?.sourceRendering,
            worker: {
              requests: window.__rvEvidence?.requests, replies: window.__rvEvidence?.replies,
              heldReply: window.__rvEvidence ? Boolean(window.__rvEvidence.heldReply) : null,
              gateReply: window.__rvEvidence?.gateReply, queuedEdits: window.__rvEvidence?.queuedEdits,
            },
          };
        }, expected)),
        keep('currentFrame', (async () => {
          const element = await page.locator('#preview').elementHandle();
          let actual: Frame | null;
          try { actual = await element.contentFrame(); } finally { await element.dispose(); }
          diagnostic.identity = {
            same: actual === retained,
            retainedAttached: page.frames().includes(retained), currentAttached: page.frames().includes(actual!),
            retainedDetached: retained.isDetached(), currentDetached: actual?.isDetached(),
            frames: page.frames().map(frame => ({
              url: frame.url(), parent: frame.parentFrame()?.url(), retained: frame === retained, current: frame === actual,
            })),
          };
          return peek(actual!);
        })()),
        keep('retainedFrame', peek(retained)),
      ]),
      new Promise<void>(resolve => {
        timer = setTimeout(() => { diagnostic.snapshotTimeout = true; resolve(); }, 2000);
      }),
    ]);
  } finally { clearTimeout(timer); }
  return structuredClone(diagnostic);
};

// Wait for the restored style's document to load and stay unchanged, then settle its charts.
// Retry once only when a navigation replaced the document during the settle call.
export async function settleRestoredPreview(page: Page, preview: Frame, previewFrame: (page: Page) => Promise<Frame>, settledCharts: (frame: Frame, count: number) => Promise<void>) {
  const waitForRestore = async () => {
    try {
      const ready = await page.waitForFunction(() => {
        const state = window.__rvEvidence.sourceRendering!;
        const quiet = document.querySelector('#preview')!.getAttribute('srcdoc')!.includes('0.985 0.004 70')
          && state.generation! > 0 && state.loaded === state.rendered
          && state.loadedGeneration === state.generation && state.loadedAt !== null
          && performance.now() - Math.max(state.changedAt!, state.loadedAt!) >= 500;
        return quiet ? state.generation! : false;
      }, null, { timeout: 60_000 });
      try { return await ready.jsonValue() as number; } finally { await ready.dispose(); }
    } catch (error) {
      if (error instanceof Error && error.name === 'TimeoutError') throw new Error('Restored preview did not stay loaded and unchanged for 500 ms', { cause: error });
      throw error;
    }
  };
  const generation = await waitForRestore();
  try {
    await settledCharts(preview, 1);
  } catch (error) {
    // A real failure stays red: only a document replaced during this call may retry once.
    if (!/Element is not attached to the DOM|Execution context was destroyed|Frame was detached|Cannot find context with specified id/i.test(String(error))
      || await page.evaluate(() => window.__rvEvidence.sourceRendering!.generation!) <= generation) throw error;
    await waitForRestore();
    preview = await previewFrame(page);
    await settledCharts(preview, 1);
  }
  return preview;
}
