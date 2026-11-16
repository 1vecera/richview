// Host reason (ARC-03): start the landing's wasm-bindgen module on the reader's first intent.
// Hovering is not intent: a cursor resting over the page gets pointer events as it loads (Firefox).
(() => {
  const staticPage = document.documentElement.dataset.landing === 'static';
  const events = staticPage
    ? ['pointerdown', 'keydown', 'click', 'focusin', 'input', 'change']
    : ['scroll', 'wheel', 'pointerdown', 'keydown', 'touchstart', 'click', 'focusin', 'input', 'change', 'hashchange'];
  let started = false;
  let observer;
  function start(event) {
    if (started) return;
    const control = event?.target?.closest?.('button, input, textarea, select, a, [data-copy]');
    if (staticPage && event?.type === 'pointerdown' && !control) return;
    if (staticPage && event?.type === 'keydown' && !control && [' ', 'ArrowDown', 'ArrowUp', 'PageDown', 'PageUp', 'Home', 'End'].includes(event.key)) return;
    started = true;
    observer?.disconnect();
    for (const name of events) window.removeEventListener(name, start, true);
    // A press on a Copy button is the gesture that starts the module, before the module can answer it.
    // Mark the button, so the module copies when it starts, still inside the press's activation window.
    const copy = event?.target?.closest?.('[data-copy]');
    const press = event?.type === 'pointerdown' || event?.type === 'click' || (event?.type === 'keydown' && (event.key === 'Enter' || event.key === ' '));
    if (copy && press) copy.setAttribute('data-copy-pressed', '');
    const demo = document.getElementById('demo');
    demo?.setAttribute('aria-busy', 'true');
    const failed = () => {
      demo?.removeAttribute('aria-busy');
      demo?.setAttribute('data-engine-failed', '');
    };
    const script = document.createElement('script');
    script.src = '/richview_site.js';
    script.onload = () => Promise.resolve().then(() => wasm_bindgen({module_or_path: '/richview_site_bg.wasm'})).catch(failed);
    script.onerror = failed;
    document.head.appendChild(script);
  }
  for (const name of events) window.addEventListener(name, start, {capture: true, passive: true});
  if (staticPage && 'IntersectionObserver' in window) {
    observer = new IntersectionObserver(entries => { if (entries.some(entry => entry.isIntersecting)) start(); });
    for (const section of document.querySelectorAll('#demo, #styles')) observer.observe(section);
  } else if (staticPage) {
    // Older browsers still get the demo and style previews as they scroll.
    window.addEventListener('scroll', start, {capture: true, passive: true, once: true});
  }
  if (!staticPage && (location.hash || window.scrollX || window.scrollY)) start();
})();
