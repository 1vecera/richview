// Host reason (ARC-03): initialize the site Wasm inside the playground's dedicated worker.
type RenderRequest = { id: number; source: string; options: string };
type RenderReply = { id: number; result: string } | { id: number; error: string };
type WorkerHost = {
  importScripts(...urls: string[]): void;
  postMessage(reply: RenderReply): void;
  onmessage: (message: MessageEvent<RenderRequest>) => Promise<void>;
};
type SiteModule = ((options: { module_or_path: string }) => Promise<unknown>) & {
  provide_font_catalog(css: string): void;
  worker_request(source: string, options: string): string;
};
declare const wasm_bindgen: SiteModule;
declare const FONT_CATALOG_URL: string;

const worker = globalThis as unknown as WorkerHost;
worker.importScripts('/richview_site.js');
const ready = Promise.all([
  wasm_bindgen({ module_or_path: '/richview_site_bg.wasm' }),
  fetch(FONT_CATALOG_URL).then(reply => reply.ok ? reply.text() : '', () => ''),
]).then(([, fonts]) => { if (fonts) wasm_bindgen.provide_font_catalog(fonts); });

worker.onmessage = async ({ data }) => {
  try {
    await ready;
    worker.postMessage({ id: data.id, result: wasm_bindgen.worker_request(data.source, data.options) });
  } catch (error) {
    worker.postMessage({ id: data.id, error: String(error) });
  }
};

export {};
