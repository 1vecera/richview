// Test-only VS Code host double: the adapter runs against the real native editor service over
// stdio. The language client is replaced by a small LSP client that forwards document events the
// way the real one does; the service, its framing and every richview/* request are real.
import type { ChildProcessByStdio } from 'node:child_process';
import type { Readable, Writable } from 'node:stream';
const assert: typeof import('node:assert/strict') = require('node:assert/strict');
// Node's private loader is used only here to intercept the two editor host imports.
const Module: typeof import('node:module') & { _load(request: string, ...args: unknown[]): unknown } = require('node:module');
const path: typeof import('node:path') = require('node:path');
const childProcess: typeof import('node:child_process') = require('node:child_process');
const { mkdirSync, readFileSync, writeFileSync }: typeof import('node:fs') = require('node:fs');
const { EventEmitter }: typeof import('node:events') = require('node:events');

const changed = new EventEmitter();

type Disposable = { dispose(): void };
type Uri = { scheme?: string; fsPath: string; toString(): string };
type Document = { languageId: string; version: number; isDirty: boolean; uri: Uri; getText(): string };
type DocumentEvent = { document: Document };
type Panel = { title: string; reveal(): void; webview: { html: string }; onDidDispose(callback: () => void): Disposable };

const registrations = new Map<string, () => Promise<unknown>>();
const notices: string[] = [];
const watchers = new Set<Disposable>();
const requests: string[] = [];
const previewRequests: unknown[] = [];
let change!: (event: DocumentEvent) => void;
let save!: (saved: { uri: Uri }) => void;
let activeEditorChanged!: (editor: DocumentEvent) => void;
let disposePanel!: () => void;
let panel!: Panel;
let inserted: string | undefined;
let secretWrites = 0;
let selectedBlock = 'heatmap';
let source = '---\ntitle: Editor contract\nversion: 0.0.1\nstyles: [scientific]\n---\n\n# Unsaved text\n';
const taskDir = path.join(process.cwd(), 'tmp', 'verify', 'editor');
mkdirSync(taskDir, { recursive: true });
writeFileSync(path.join(taskDir, 'editor-unsaved.rmd'), '# Persisted source\n');
const file = path.join(taskDir, 'editor-unsaved.rmd');
const document: Document = {
  languageId: 'richview', version: 1, isDirty: true,
  uri: { scheme: 'file', fsPath: file, toString: () => 'file://' + file },
  getText: () => source,
};
const disposable = () => ({ dispose() {} });
const config: Record<string, string> = { binary: process.env.RICHVIEW_EDITOR_BIN || '', server: '' };
const host = {
  workspace: {
    isTrusted: true, textDocuments: [document],
    onDidGrantWorkspaceTrust: () => disposable(),
    getConfiguration: () => ({ get: (key: string) => config[key] }),
    createFileSystemWatcher: () => {
      const watcher = { onDidCreate: () => disposable(), onDidChange: () => disposable(), onDidDelete: () => disposable(), dispose() { watchers.delete(watcher); } };
      watchers.add(watcher);
      return watcher;
    },
    onDidChangeTextDocument: (fn: (event: DocumentEvent) => void) => { change = fn; return disposable(); },
    onDidSaveTextDocument: (fn: (saved: { uri: Uri }) => void) => { save = fn; return disposable(); },
  },
  commands: { registerCommand: (name: string, fn: () => Promise<unknown>) => { registrations.set(name, fn); return disposable(); }, executeCommand: async () => {} },
  window: {
    onDidChangeActiveTextEditor: (fn: (editor: DocumentEvent) => void) => { activeEditorChanged = fn; return disposable(); },
    activeTextEditor: { document, insertSnippet: async (snippet: { value: string }) => { inserted = snippet.value; } },
    createWebviewPanel: () => {
      let html = '';
      panel = { title: '', reveal() {}, webview: {
        get html() { return html; },
        set html(value) { html = value; changed.emit('change'); },
      }, onDidDispose: fn => { disposePanel = fn; return disposable(); } };
      return panel;
    },
    showQuickPick: async (items: { id: string; body: string }[]) => items.find(item => item.id === selectedBlock),
    showInformationMessage: async (message: string) => { notices.push(message); },
    showWarningMessage: async (message: string) => { notices.push(message); },
    showErrorMessage: async (message: string) => { notices.push(message); },
    showInputBox: async () => undefined,
  },
  ViewColumn: { Beside: 2 },
  RelativePattern: class { constructor(base: string, pattern: string) { Object.assign(this, { base, pattern }); } },
  SnippetString: class { value: string; constructor(value: string) { this.value = value; } },
  env: { clipboard: { writeText: async () => {} }, openExternal: async () => { throw new Error('No outward action is implicit.'); } },
  Uri: { parse: (value: string) => value, file: (fsPath: string) => ({ fsPath }) },
};

// A minimal LSP client for the real service: initialize, document sync, sendRequest.
class LanguageClient {
  server: { command: string; args: string[] };
  next: number;
  pending: Map<number, { resolve(value: unknown): void; reject(reason: unknown): void }>;
  buffer: Buffer;
  published: unknown[];
  child?: ChildProcessByStdio<Writable, Readable, null>;

  constructor(id: string, name: string, server: { command: string; args: string[] }) {
    current = this;
    this.server = server; this.next = 0; this.pending = new Map(); this.buffer = Buffer.alloc(0);
    this.published = [];
  }
  write(message: object) {
    const body = Buffer.from(JSON.stringify(message));
    this.child!.stdin.write(`Content-Length: ${body.length}\r\n\r\n`); this.child!.stdin.write(body);
  }
  async start() {
    this.child = childProcess.spawn(this.server.command, this.server.args, { stdio: ['pipe', 'pipe', 'inherit'] });
    this.child.stdout.on('data', (chunk: Buffer) => this.read(chunk));
    await this.sendRequest('initialize', { capabilities: {} });
    for (const open of host.workspace.textDocuments) {
      this.write({ jsonrpc: '2.0', method: 'textDocument/didOpen', params: { textDocument: { uri: open.uri.toString(), languageId: 'richview', version: open.version, text: open.getText() } } });
    }
    // The real client sends every edit and save before any later request.
    const original = host.workspace.onDidChangeTextDocument;
    host.workspace.onDidChangeTextDocument = fn => {
      const forward = ({ document: edited }: DocumentEvent) => {
        this.write({ jsonrpc: '2.0', method: 'textDocument/didChange', params: { textDocument: { uri: edited.uri.toString(), version: edited.version }, contentChanges: [{ text: edited.getText() }] } });
        return fn({ document: edited });
      };
      return original(forward);
    };
  }
  stop() { this.child?.kill(); }
  sendNotification(method: string, params: unknown) {
    this.write({ jsonrpc: '2.0', method, params });
    return Promise.resolve();
  }
  sendRequest(method: string, params: unknown) {
    const id = ++this.next;
    requests.push(method);
    if (method === 'richview/preview') previewRequests.push(params);
    return new Promise<unknown>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.write({ jsonrpc: '2.0', id, method, params });
    });
  }
  read(chunk: Buffer) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    for (;;) {
      const split = this.buffer.indexOf('\r\n\r\n');
      if (split < 0) return;
      const length = Number(/Content-Length: (\d+)/i.exec(this.buffer.subarray(0, split).toString())![1]);
      if (this.buffer.length < split + 4 + length) return;
      const message: { method?: string; params?: unknown; id?: number; error?: { message: string }; result?: unknown } = JSON.parse(this.buffer.subarray(split + 4, split + 4 + length).toString());
      this.buffer = this.buffer.subarray(split + 4 + length);
      if (message.method === 'textDocument/publishDiagnostics') this.published.push(message.params);
      const pending = this.pending.get(message.id!);
      if (!pending) continue;
      this.pending.delete(message.id!);
      if (message.error) pending.reject(new Error(message.error.message)); else pending.resolve(message.result);
      queueMicrotask(() => changed.emit('change'));
    }
  }
}
let clients = 0; let current!: LanguageClient;
const original = Module._load;
Module._load = function (request: string, ...rest: unknown[]) {
  if (request === 'vscode') return host;
  if (request === 'vscode-languageclient/node') return { LanguageClient: class extends LanguageClient { constructor(...args: ConstructorParameters<typeof LanguageClient>) { super(...args); clients++; } } };
  return original.call(this, request, ...rest);
};
const context = {
  extensionPath: path.join(process.cwd(), 'packages/vscode'), subscriptions: [] as Disposable[],
  secrets: { get: async () => undefined, store: async () => { secretWrites++; throw new Error('No credential write is implicit.'); } },
};
const extension: { activate(hostContext: typeof context): void } = require('../packages/vscode/extension.cts');
const wait = (predicate: () => boolean) => {
  if (predicate()) return Promise.resolve();
  return new Promise<void>((resolve, reject) => {
    const finish = (error?: Error) => {
      clearTimeout(timer);
      changed.off('change', check);
      if (error) reject(error); else resolve();
    };
    const check = () => { if (predicate()) finish(); };
    const timer = setTimeout(() => finish(new Error('Editor response timed out')), 6000);
    changed.on('change', check);
    check();
  });
};

(async () => {
  try {
    extension.activate(context);
    await wait(() => registrations.size === 4 && clients === 1);
    assert.deepEqual([...registrations.keys()].sort(), ['richview.export', 'richview.insertBlock', 'richview.preview', 'richview.publish']);

    // Preview renders the unsaved buffer with the real runtime embedded by the native editor.
    await registrations.get('richview.preview')!();
    assert.equal(watchers.size, 1, 'The active preview owns its dependency watcher');
    assert.match(panel.webview.html, /Unsaved text/);
    assert.equal(Object.hasOwn(previewRequests.at(-1) as object, 'runtimeUrl'), false);
    assert.ok(panel.webview.html.includes('atob('));
    assert.ok(!panel.webview.html.includes('vscode-resource'));
    assert.match(panel.title, /RichView Preview/);

    // Edits reach the service before the render request; twenty rapid edits end on the last.
    for (let i = 0; i < 20; i++) { source = source.replace(/Unsaved text(?: \d+)?/, `Unsaved text ${i}`); document.version++; change({ document }); }
    await wait(() => current.pending.size === 0 && panel.webview.html.includes('Unsaved text 19'));
    assert.match(panel.webview.html, /Unsaved text 19/);

    // A saved file in the report folder refreshes the bound preview.
    writeFileSync(path.join(taskDir, 'data.csv'), 'x,value\nJan,123\nFeb,234\n');
    source += '\n```chart\ntype: bar\ntitle: Local dependency\ndata: {csv: data.csv}\nencoding: {x: x, y: value}\n```\n';
    document.version++; change({ document });
    await wait(() => panel.webview.html.includes('Local dependency'));
    writeFileSync(path.join(taskDir, 'data.csv'), 'x,value\nJan,321\nFeb,234\n');
    save({ uri: { fsPath: path.join(taskDir, 'data.csv'), toString: () => 'file://data.csv' } });
    await wait(() => /"value":"321"/.test(panel.webview.html));

    // Rebinding to another report and closing the panel release the old watchers.
    const otherDir = taskDir + '-rebound'; mkdirSync(otherDir, { recursive: true });
    const otherPath = path.join(otherDir, 'other.rmd'); writeFileSync(otherPath, '# Rebound preview\n');
    const other = { ...document, uri: { scheme: 'file', fsPath: otherPath, toString: () => 'file://' + otherPath }, getText: () => '# Rebound preview\n' };
    host.workspace.textDocuments.push(other);
    host.window.activeTextEditor.document = other;
    // The service learns of the second document the way the client would: by an open event.
    current.write({ jsonrpc: '2.0', method: 'textDocument/didOpen', params: { textDocument: { uri: other.uri.toString(), languageId: 'richview', version: 1, text: other.getText() } } });
    activeEditorChanged({ document: other });
    await wait(() => panel.webview.html.includes('Rebound preview'));
    host.window.activeTextEditor.document = document;
    assert.equal(watchers.size, 1, 'Switching reports releases the old watcher');
    disposePanel();
    assert.equal(watchers.size, 0, 'Closing the preview releases its watcher');

    // One Insert Block pick, from the service's own catalogue.
    await registrations.get('richview.insertBlock')!();
    assert.match(inserted!, /type: heatmap/);
    selectedBlock = 'histogram'; inserted = undefined;
    await registrations.get('richview.insertBlock')!();
    assert.match(inserted!, /type: histogram/);
    selectedBlock = 'table'; inserted = undefined;
    await registrations.get('richview.insertBlock')!();
    assert.match(inserted!, /```table/);

    // Export refuses an unsaved document; publish needs a configured server and never writes a secret.
    await registrations.get('richview.export')!();
    assert.ok(notices.some(m => m.includes('Save the report')));
    assert.ok(!requests.includes('richview/export'), 'An unsaved document is never exported');
    await registrations.get('richview.publish')!();
    document.isDirty = false;
    await registrations.get('richview.publish')!();
    assert.ok(notices.some(m => m.includes('richview.server')));
    assert.ok(!requests.includes('richview/publish'));
    assert.equal(secretWrites, 0);

    // Exercise the extension's configuration and preview channel before exporting. The removed
    // preview-only setting must not override authored styles or reach the saved export.
    config.previewTheme = 'dark';
    source = source.replace('Unsaved text 19', 'Saved heading');
    writeFileSync(file, source);
    document.version++; change({ document });
    await registrations.get('richview.preview')!();
    assert.match(panel.webview.html, /Saved heading/);
    assert.match(panel.webview.html, /"value":"321"/);
    await registrations.get('richview.export')!();
    assert.ok(notices.some(m => m.startsWith('Exported: ') && m.endsWith('Rendered/editor-unsaved.html')), notices.join('\n'));
    const output = path.join(taskDir, 'Rendered', 'editor-unsaved.html');
    const exported = readFileSync(output);
    const cli = process.env.RICHVIEW_CLI || path.join(process.cwd(), 'target/release/richview');
    const generated = childProcess.spawnSync(cli, ['generate', file, '--format', 'json'], { encoding: 'utf8', timeout: 30_000 });
    assert.equal(generated.status, 0, generated.stderr || generated.stdout);
    assert.equal(JSON.parse(generated.stdout).output, output);
    assert.ok(readFileSync(output).equals(exported), 'Extension export after configured preview and CLI generate write identical bytes');
    assert.equal(readFileSync(file, 'utf8'), source, 'Preview, export and generate preserve authored source');

    const evidence = {
      host: 'VS Code API double with the real editor service over stdio',
      checks: ['four commands only: preview, insert block, export, publish', 'unsaved source preview embeds the real native runtime without a webview resource URL', 'edits reach the service before the render; 20 rapid edits end on the last', 'saved local data refreshes the bound report', 'preview rebind and close release the dependency watcher', 'one Insert Block pick from the service catalogue', 'export refuses an unsaved document and writes Rendered/ for a saved one', 'legacy preview configuration through the extension leaves export byte-identical to CLI generate and source unchanged', 'publish needs a named server and writes no credential implicitly'],
      requests: [...new Set(requests)],
      commands: [...registrations.keys()],
    };
    mkdirSync('tmp/verify/evidence', { recursive: true });
    writeFileSync('tmp/verify/evidence/editor.json', JSON.stringify(evidence, null, 2));
    console.log(JSON.stringify(evidence, null, 2));
  } finally {
    for (const subscription of context.subscriptions) subscription.dispose?.();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
