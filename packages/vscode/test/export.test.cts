// VS Code host API contract (ARC-03): execute the shipped adapter without a desktop or renderer.
// Service replies are protocol inputs only; native output bytes are checked by verify-editor.cts.
const assert: typeof import('node:assert/strict') = require('node:assert/strict');
const fs: typeof import('node:fs') = require('node:fs');
const path: typeof import('node:path') = require('node:path');
const test: typeof import('node:test').test = require('node:test');
const vm: typeof import('node:vm') = require('node:vm');
const { pathToFileURL }: typeof import('node:url') = require('node:url');
const { createRequire, stripTypeScriptTypes }: typeof import('node:module') = require('node:module');

type FixtureUri = { scheme: string; fsPath?: string; toString(): string };
type Document = { languageId: string; isDirty: boolean; isUntitled: boolean; uri: FixtureUri };
type Disposable = { dispose(): void };
type Context = { extensionPath: string; subscriptions: Disposable[] };
type Request = { method: string; params: unknown };
type Command = (resource?: FixtureUri) => Promise<unknown>;
type ManifestCommand = { command: string; when: string };

const root = path.resolve(__dirname, '..');
const load = createRequire(path.join(root, 'extension.cts'));
const output = path.resolve('/service-selected/Rendered/an output.html');
const disposable = () => ({ dispose() {} });
function document(file = '/reports/active.rmd', extra: Partial<Document> = {}): Document {
  return {
    languageId: 'richview', isDirty: false, isUntitled: false,
    uri: { scheme: 'file', fsPath: file, toString: () => pathToFileURL(file).href },
    ...extra,
  };
}
function adapter({ active = document(), choice, failure, opened = [] }: { active?: Document | null; choice?: string; failure?: Error; opened?: Document[] } = {}) {
  const commands = new Map<string, Command>(), requests: Request[] = [];
  const notices: { message: string; actions: string[] }[] = [];
  const warnings: string[] = [], errors: string[] = [], opens: (string | undefined)[] = [];
  const executed: [command: string, uri: FixtureUri][] = [];
  const host = {
    workspace: {
      isTrusted: true,
      getConfiguration: () => ({ get: (key: string) => ({ previewTheme: 'dark', styles: ['playful'] } as Record<string, unknown>)[key] }),
      onDidChangeTextDocument: disposable, onDidSaveTextDocument: disposable,
      openTextDocument: async (uri: FixtureUri) => {
        opens.push(uri.fsPath);
        const result = opened.find(doc => doc.uri.fsPath === uri.fsPath);
        assert.ok(result, `Unexpected document open: ${uri.fsPath}`);
        return result;
      },
    },
    commands: {
      registerCommand: (id: string, fn: Command) => { commands.set(id, fn); return disposable(); },
      executeCommand: async (...args: [string, FixtureUri]) => executed.push(args),
    },
    window: {
      activeTextEditor: active && { document: active },
      onDidChangeActiveTextEditor: disposable,
      showInformationMessage: async (message: string, ...actions: string[]) => { notices.push({ message, actions }); return choice; },
      showWarningMessage: async (message: string) => warnings.push(message),
      showErrorMessage: async (message: string) => errors.push(message),
    },
    Uri: { file: (file: string) => ({ scheme: 'file', fsPath: file, toString: () => pathToFileURL(file).href }) },
  };
  class LanguageClient {
    start() { return Promise.resolve(); }
    stop() {}
    async sendRequest(method: string, params: unknown) {
      requests.push({ method, params });
      if (failure) throw failure;
      assert.equal(method, 'richview/export');
      return { output };
    }
  }
  const module: { exports: { activate?: (context: Context) => void } } = { exports: {} };
  vm.runInNewContext(stripTypeScriptTypes(fs.readFileSync(path.join(root, 'extension.cts'), 'utf8')), {
    module, process, setTimeout, clearTimeout,
    __filename: path.join(root, 'extension.cts'), __dirname: root,
    require(name: string) {
      if (name === 'vscode') return host;
      if (name === 'vscode-languageclient/node') return { LanguageClient };
      return load(name);
    },
  }, { filename: path.join(root, 'extension.cts') });
  const context: Context = { extensionPath: root, subscriptions: [] };
  module.exports.activate!(context);
  return { commands, requests, notices, warnings, errors, executed, opens,
    dispose: () => context.subscriptions.forEach(subscription => subscription.dispose()),
    // Compare cross-realm protocol objects as the JSON that the language client transports.
    sent: (): Request[] => JSON.parse(JSON.stringify(requests)),
  };
}

test('dirty and new untitled reports refuse export before requesting a saved-file operation', async t => {
  for (const active of [document('/reports/dirty.rmd', { isDirty: true }), document('Untitled-1', {
    isUntitled: true, uri: { scheme: 'untitled', toString: () => 'untitled:Untitled-1' },
  })]) {
    const host = adapter({ active });
    t.after(host.dispose);
    await host.commands.get('richview.export')!();
    assert.deepEqual(host.sent(), [], 'Unsaved content cannot select an export operation');
    assert.equal(host.warnings.length, 1);
    assert.match(host.warnings[0], /Save the report before export/);
    assert.deepEqual(host.notices, []);
    assert.deepEqual(host.executed, []);
  }
});

test('saved export sends only its file URI and offers Reveal for the service-selected output', async t => {
  for (const choice of ['Reveal', undefined]) {
    const active = document('/reports/a saved report.rmd');
    const host = adapter({ active, choice });
    t.after(host.dispose);
    await host.commands.get('richview.export')!();
    assert.deepEqual(host.sent(), [{ method: 'richview/export', params: { uri: active.uri.toString() } }]);
    assert.deepEqual(host.notices, [{ message: `Exported: ${output}`, actions: ['Reveal'] }]);
    assert.deepEqual(host.errors, []);
    if (choice === 'Reveal') {
      assert.equal(host.executed.length, 1);
      const [command, uri] = host.executed[0];
      assert.equal(command, 'revealFileInOS');
      assert.equal(uri.scheme, 'file');
      assert.equal(uri.fsPath, output);
      assert.equal(uri.toString(), pathToFileURL(output).href);
    } else assert.deepEqual(host.executed, [], 'Dismissing the notice cannot open an OS window');
  }
});

test('Explorer export targets the chosen report instead of the active editor', async t => {
  const chosen = document('/reports/chosen.rmd');
  const host = adapter({ active: document('/reports/active.rmd', { isDirty: true }), opened: [chosen] });
  t.after(host.dispose);
  await host.commands.get('richview.export')!(chosen.uri);
  assert.deepEqual(host.opens, [chosen.uri.fsPath]);
  assert.deepEqual(host.sent(), [{ method: 'richview/export', params: { uri: chosen.uri.toString() } }]);
  assert.deepEqual(host.warnings, []);
});

test('export outside a RichView document does not ask the service or reveal a file', async t => {
  for (const active of [null, document('/reports/readme.md', { languageId: 'markdown' })]) {
    const host = adapter({ active });
    t.after(host.dispose);
    await host.commands.get('richview.export')!();
    assert.deepEqual(host.sent(), []);
    assert.deepEqual(host.notices, []);
    assert.deepEqual(host.executed, []);
  }
});

test('a refused native export surfaces the failure without a success notice or Reveal', async t => {
  // The service's error carries the failed call's result object in its data (TYP-54).
  const failure = Object.assign(new Error('report.source: Fix the saved report before export.'), {
    code: -32000,
    data: { ok: false, status: 'issues', output: null, diagnostics: [
      { level: 'error', code: 'report.source', message: 'Fix the saved report before export.' },
    ] },
  });
  const host = adapter({ choice: 'Reveal', failure });
  t.after(host.dispose);
  await host.commands.get('richview.export')!();
  assert.equal(host.sent().length, 1);
  assert.deepEqual(host.errors, ['report.source: Fix the saved report before export.']);
  assert.deepEqual(host.notices, []);
  assert.deepEqual(host.executed, []);
});

test('the contributed file commands match activation, have no Validate, and are restricted to rmd contexts', t => {
  const host = adapter();
  t.after(host.dispose);
  const manifest: { contributes: { commands: ManifestCommand[]; menus: Record<string, ManifestCommand[]> } } = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
  const commands = manifest.contributes.commands.map(item => item.command).sort();
  assert.deepEqual(commands, ['richview.export', 'richview.insertBlock', 'richview.preview', 'richview.publish']);
  assert.deepEqual([...host.commands.keys()].sort(), commands);
  for (const [menu, items] of Object.entries(manifest.contributes.menus)) {
    for (const item of items) {
      assert.ok(host.commands.has(item.command), `${menu}: unregistered command ${item.command}`);
      assert.equal(item.when.replace(/\s+/g, ''), menu === 'explorer/context' ? 'resourceExtname==.rmd' : 'editorLangId==richview');
    }
  }
  assert.deepEqual(manifest.contributes.menus.commandPalette.map(item => item.command).sort(), commands);
});
