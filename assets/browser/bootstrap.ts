// Host reason (ARC-03): start the correct wasm-bindgen module on a site or docs page.
type Start = (options: { module_or_path: string }) => Promise<unknown>;
declare const richview_docs: Start;
declare const wasm_bindgen: Start;

const docs = document.body?.dataset.richviewPage === 'docs' || document.body?.classList.contains('install-page');
(docs ? richview_docs : wasm_bindgen)({ module_or_path: docs ? '/richview_docs_bg.wasm' : '/richview_site_bg.wasm' });

export {};
