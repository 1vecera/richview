// Host reason (ARC-03): execute the docs browser check's actual source extractor without a browser.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import { readTestSource } from './test-source.mts';

const root = new URL('../../../', import.meta.url);
const owner = readTestSource(new URL('scripts/verify-docs.ts', root));
const start = owner.indexOf('const richTags');
const end = owner.indexOf('\nasync function startServer(', start);
assert.ok(start >= 0 && end > start, 'Locate the shipping extractor without importing the browser harness');
// The slice evaluates to the production string-to-examples extractor.
const extract: (source: string) => Iterable<string> = vm.runInNewContext(`${owner.slice(start, end)}\ncodeExamples;`);
const examples = (source: string) => Array.from(extract(source));

test('the shipped guides keep their CSV and nested chart examples as code', () => {
  const data = readFileSync(new URL('docs/public/data.md', root), 'utf8');
  const dataExamples = examples(data);
  assert.ok(dataExamples.includes('data:\n  csv: data/visits.csv\n'), 'The real CSV snippet must not become following prose');
  assert.ok(!dataExamples.some(code => code.includes('Use `data.json`')), 'Closing the CSV fence must leave the following prose outside code');
  const source = readFileSync(new URL('docs/public/authoring.md', root), 'utf8');
  const actual = examples(source);
  const match = source.match(/````rmd\n([\s\S]+?)\n````/);
  assert.ok(match, 'The tutorial contains a four-backtick RMD fence');
  const tutorial = match[1] + '\n';
  assert.equal(actual[0], tutorial, 'The four-backtick tutorial retains its nested chart fence byte for byte');
});

test('complete info strings preserve the full ordered code array and closing boundaries', () => {
  const source = [
    '```yaml chart-data context=examples/first-report',
    'data:',
    '  csv: data/orders.csv',
    '```  \t',
    'This prose is not code.',
    '```shell-session context=example',
    '$ richview check report.rmd',
    '```yaml this is content, not a closing fence',
    '````',
    '```',
    'plain code',
    '```',
  ].join('\n');
  assert.deepEqual(examples(source), [
    'data:\n  csv: data/orders.csv\n',
    '$ richview check report.rmd\n```yaml this is content, not a closing fence\n',
    'plain code\n',
  ]);
});

test('only the first info token classifies rich blocks and shorter nested fences stay literal', () => {
  const source = [
    '```chart context=example',
    'type: bar',
    '```',
    '```charting context=example',
    'not a rich tag',
    '```',
    '````rmd context=example',
    '```chart',
    'type: bar',
    '```',
    '````',
    '```  table context=example',
    'data: {inline: []}',
    '```',
  ].join('\n');
  assert.deepEqual(examples(source), [
    'not a rich tag\n',
    '```chart\ntype: bar\n```\n',
  ]);
});

test('copyable code includes the installer only when enabled and rejects missing or extra blocks', () => {
  const begin = owner.indexOf('function docsSource(');
  const finish = owner.indexOf('\nasync function startServer(', begin);
  assert.ok(begin >= 0 && finish > begin);
  const source = '{{native-install}}\n```sh\necho report\n```\n';
  const installer = 'curl -fsSL https://github.com/1vecera/richview/releases/latest/download/install.sh | sh\nexport PATH="$HOME/.richview/bin:$PATH"\nrichview --help\n';
  for (const installEnabled of [false, true]) {
    const check: (actual: string[], source: string, name: string) => void = vm.runInNewContext(`${owner.slice(begin, finish)}\n(actual, source, name) => assertCopyableCode(Array.from(actual), source, name);`, {
      assert, installEnabled, repositoryPublic: installEnabled,
      siteModeExpectations: () => ({prompt: ''}), release: {},
    });
    const blocks = [...(installEnabled ? [installer] : []), 'echo report\n'];
    assert.doesNotThrow(() => check(blocks, source, 'first-report'));
    assert.throws(() => check(blocks.slice(1), source, 'first-report'), /changed copyable code/);
    assert.throws(() => check([...blocks, 'extra\n'], source, 'first-report'), /changed copyable code/);
  }
});
