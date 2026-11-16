// Host reason (ARC-03): the gate's Node tests read the guide and VS Code manifest to catch
// user-facing configuration names that stopped existing, without compiling or starting a host.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const root = new URL('../../../', import.meta.url);

test('the tool guide uses the editor settings and language IDs the extension declares', () => {
  const manifest = JSON.parse(readFileSync(new URL('packages/vscode/package.json', root), 'utf8'));
  const config = manifest.contributes.configuration;
  const groups = Array.isArray(config) ? config : [config];
  const declared = new Set(groups.flatMap(group => Object.keys(group.properties ?? {})));
  const guide = readFileSync(new URL('docs/public/tools.md', root), 'utf8');
  const names = [...guide.matchAll(/`(richview\.[A-Za-z][A-Za-z0-9_.-]*)`/g)].map(match => match[1]);
  assert.deepEqual(names.filter(name => !declared.has(name)), [], 'the guide must not advertise an editor setting the shipped manifest lacks');
  const languages = new Set(manifest.contributes.languages.map(language => language.id));
  for (const [, source] of guide.matchAll(/```json\n([\s\S]*?)\n```/g)) {
    for (const [key, settings] of Object.entries(JSON.parse(source))) {
      const language = /^\[([^\]]+)\]$/.exec(key)?.[1];
      if (!language) continue;
      assert.ok(languages.has(language), `${key} must select a declared language`);
      assert.equal(settings['editor.defaultFormatter'], `${manifest.publisher}.${manifest.name}`, 'the example must select this extension as formatter');
    }
  }
});
