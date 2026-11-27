// The same three credited assets must remain readable report media online and offline; the App Store
// ratings are a live chart since 9 November 2026, so they are counted with the charts.
import assert from 'node:assert/strict';
import { join } from 'node:path';
import type { Scope } from './browser-harness.ts';

export async function captureFlagshipImages(scope: Scope, directory: string, label: string) {
  const figures = scope.locator('.rv-report figure.rv-image');
  const expected = [
    { id: 'I-COVER', width: 1200, height: 630, credit: 'Original RichView graphic', license: 'MIT' },
    { id: 'I-CODEX', width: 1898, height: 1190, credit: 'OpenAI', license: 'Apache 2.0' },
    { id: 'I-COMPUTER-USE', width: 1200, height: 827, credit: 'Anthropic', license: 'MIT' },
  ];
  assert.equal(await figures.count(), expected.length);
  const evidence = [];
  for (const [index, asset] of expected.entries()) {
    const figure = figures.nth(index);
    await figure.scrollIntoViewIfNeeded();
    const value = await figure.evaluate(async figure => {
      const image = figure.querySelector('img')!;
      // Scrolling a lazy image into view can precede its request, especially in Firefox.
      // Wait for the actual load before decode; broken or stalled assets still fail.
      if (!image.complete) await new Promise<void>((resolve, reject) => {
        const finish = (error?: Error) => {
          clearTimeout(timeout);
          image.removeEventListener('load', loaded);
          image.removeEventListener('error', failed);
          error ? reject(error) : resolve();
        };
        const loaded = () => finish();
        const failed = () => finish(new Error(`Image failed to load: ${image.alt}`));
        const timeout = setTimeout(() => finish(new Error(`Image load timed out: ${image.alt}`)), 30_000);
        image.addEventListener('load', loaded, { once: true });
        image.addEventListener('error', failed, { once: true });
      });
      await image.decode();
      const box = image.getBoundingClientRect();
      const caption = figure.querySelector('figcaption');
      return { width: image.naturalWidth, height: image.naturalHeight, alt: image.alt, caption: caption?.textContent, captionHeight: caption?.getBoundingClientRect().height, displayedWidth: box.width, displayedHeight: box.height, viewport: innerWidth, left: box.left, right: box.right };
    });
    assert.equal(value.width, asset.width, asset.id); assert.equal(value.height, asset.height, asset.id);
    assert.ok(value.alt.length > 50 && value.caption?.includes(asset.credit) && value.caption.includes(asset.license), asset.id);
    assert.ok(value.captionHeight! > 10 && value.displayedWidth > 250 && value.displayedHeight > 20, `${asset.id}: hidden media/credit`);
    assert.ok(value.left >= -1 && value.right <= value.viewport + 1, `${asset.id}: horizontal overflow`);
    assert.ok(Math.abs(value.displayedWidth / value.displayedHeight - asset.width / asset.height) < 0.01, `${asset.id}: image aspect ratio changed`);
    await figure.screenshot({ path: join(directory, `images-${label}-${asset.id}.png`) });
    evidence.push({ id: asset.id, ...value });
  }
  return { label, assets: evidence };
}
