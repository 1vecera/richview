// Host reason (ARC-03): start the externally loaded report runtime and surface startup errors.
declare function richview_runtime(options: { module_or_path: string }): Promise<unknown>;

let leaving = false;
addEventListener('pagehide', () => { leaving = true; }, { once: true });
richview_runtime({ module_or_path: '/richview_runtime_bg.wasm' }).catch((error: unknown) => {
  if (!leaving || (error as { name?: string } | undefined)?.name !== 'AbortError') throw error;
});

export {};
