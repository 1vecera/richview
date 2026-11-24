// VS Code requires a JavaScript activation module (ARC-03: host API glue). Every behaviour lives in
// richview-editor, which this module starts as a language server and asks for the rich requests.
const vscode = require('vscode');
const path = require('node:path');
const { LanguageClient } = require('vscode-languageclient/node');

const LANGUAGE = 'richview';

function activate(context) {
  let client;
  let ready;
  let panel;
  let bound;
  let boundDocument;
  let dependencyRoot;
  let dependencyTimer;
  let dependencySubscriptions = [];

  const start = () => {
    if (client) return;
    const command = vscode.workspace.getConfiguration('richview').get('binary') ||
      path.join(context.extensionPath, 'bin', process.platform === 'win32' ? 'richview-editor.exe' : 'richview-editor');
    client = new LanguageClient('richview', 'RichView', { command, args: [] },
      { documentSelector: [{ language: LANGUAGE }] });
    // Listeners that send requests register after the client's own document sync, so the service
    // has every edit before it is asked to render.
    ready = client.start();
    ready.then(() => {
      context.subscriptions.push(
        vscode.workspace.onDidChangeTextDocument(({ document }) => {
          if (panel && bound === document.uri.toString()) refresh(document).catch(report);
        }),
        vscode.workspace.onDidSaveTextDocument(document => dependencyChanged(document.uri)),
        vscode.window.onDidChangeActiveTextEditor(editor => {
          if (panel && editor?.document.languageId === LANGUAGE) {
            bindPreview(editor.document);
            refresh(editor.document).catch(report);
          }
        }));
    }, serviceDown);
    // A service that stops later is reported the same way, once.
    client.onDidChangeState?.(({ newState }) => { if (newState === 1) serviceDown(); });
  };
  let serviceReported = false;
  const serviceDown = () => {
    if (serviceReported) return;
    serviceReported = true;
    vscode.window.showErrorMessage('editor.service: the RichView editor service is not running. Set richview.binary to a richview-editor binary, or reinstall the extension.');
  };
  const report = error => vscode.window.showErrorMessage(messageOf(error));
  // A failed request carries its result object, diagnostics included, in the error's data (TYP-54).
  const findingsOf = error => Array.isArray(error?.data?.diagnostics) ? error.data.diagnostics : [];
  const messageOf = error => {
    const findings = findingsOf(error);
    return findings.length ? findings.map(f => `${f.code}: ${f.message}`).join('\n') : (error?.message ?? String(error));
  };
  const active = () => {
    const document = vscode.window.activeTextEditor?.document;
    return document?.languageId === LANGUAGE ? document : undefined;
  };
  const guarded = handler => async (...args) => {
    try { await handler(...args); } catch (error) { report(error); }
  };
  // The document a command acts on: the file right-clicked in the explorer, else the open one.
  const target = async resource => resource?.fsPath && resource.fsPath.endsWith('.rmd')
    ? vscode.workspace.openTextDocument(resource)
    : active();
  const register = (id, handler) => context.subscriptions.push(vscode.commands.registerCommand(id, guarded(handler)));

  // Requests wait for the server's start, so the service has every open document first.
  const ask = async (method, params) => {
    if (!ready) throw new Error('Trust the workspace before running the RichView editor service.');
    await ready;
    return client.sendRequest(method, params);
  };
  const refresh = async document => {
    const version = document.version;
    const result = await ask('richview/preview', { uri: document.uri.toString() });
    if (result.cancelled) return;
    if (panel && bound === document.uri.toString() && document.version === version) {
      panel.webview.html = result.html;
      panel.title = result.title;
    }
  };
  const stopDependencies = () => {
    clearTimeout(dependencyTimer); dependencyTimer = undefined;
    for (const subscription of dependencySubscriptions) subscription.dispose();
    dependencySubscriptions = []; dependencyRoot = undefined;
  };
  const dependencyChanged = (uri, type = 2) => {
    if (!dependencyRoot || !panel || !boundDocument || boundDocument.isClosed || !vscode.workspace.isTrusted) return;
    const relative = path.relative(dependencyRoot, uri.fsPath);
    if (!relative || path.isAbsolute(relative) || relative === '..' || relative.startsWith('..' + path.sep) ||
        relative.split(path.sep)[0] === 'Rendered' || uri.toString() === bound) return;
    clearTimeout(dependencyTimer);
    dependencyTimer = setTimeout(() => {
      dependencyTimer = undefined;
      const document = boundDocument;
      if (!panel || !document || document.isClosed || !vscode.workspace.isTrusted) return;
      client.sendNotification('workspace/didChangeWatchedFiles', { changes: [{ uri: uri.toString(), type }] })
        .then(() => refresh(document)).catch(report);
    }, 150);
  };
  const bindPreview = document => {
    bound = document.uri.toString();
    boundDocument = document;
    const folder = document.uri.scheme === 'file' ? path.dirname(document.uri.fsPath) : undefined;
    if (folder === dependencyRoot) return;
    stopDependencies();
    if (!folder) return;
    dependencyRoot = folder;
    const watcher = vscode.workspace.createFileSystemWatcher(new vscode.RelativePattern(folder, '**/*'));
    dependencySubscriptions = [watcher.onDidCreate(uri => dependencyChanged(uri, 1)), watcher.onDidChange(uri => dependencyChanged(uri, 2)),
      watcher.onDidDelete(uri => dependencyChanged(uri, 3)), watcher];
  };

  register('richview.preview', async () => {
    const document = active();
    if (!document) return vscode.window.showInformationMessage('Open a .rmd file first.');
    if (!panel) {
      panel = vscode.window.createWebviewPanel('richview.preview', 'RichView Preview', vscode.ViewColumn.Beside,
        { enableScripts: true, retainContextWhenHidden: true, localResourceRoots: [] });
      panel.onDidDispose(() => { stopDependencies(); panel = undefined; bound = undefined; boundDocument = undefined; }, null, context.subscriptions);
    } else panel.reveal(vscode.ViewColumn.Beside);
    bindPreview(document);
    await refresh(document);
  });
  register('richview.insertBlock', async () => {
    const editor = vscode.window.activeTextEditor;
    if (editor?.document.languageId !== LANGUAGE) return;
    const choice = await vscode.window.showQuickPick(await ask('richview/snippets', {}),
      { placeHolder: 'Block to insert' });
    if (choice) await editor.insertSnippet(new vscode.SnippetString(choice.body));
  });
  register('richview.export', async resource => {
    const document = await target(resource);
    if (!document) return;
    if (document.isUntitled || document.isDirty) return vscode.window.showWarningMessage('Save the report before export so Rendered/ holds the same source and output.');
    const result = await ask('richview/export', { uri: document.uri.toString() });
    const choice = await vscode.window.showInformationMessage('Exported: ' + result.output, 'Reveal');
    if (choice === 'Reveal') await vscode.commands.executeCommand('revealFileInOS', vscode.Uri.file(result.output));
  });
  register('richview.publish', async resource => {
    const document = await target(resource);
    if (!document) return;
    if (document.isDirty) return vscode.window.showWarningMessage('Save the report before publishing it.');
    const settings = vscode.workspace.getConfiguration('richview');
    let server = settings.get('server');
    if (!server) {
      // Asked for once and kept in the setting; there is no default server (FEA-SF-12).
      server = await vscode.window.showInputBox({
        prompt: 'URL of your RichView server',
        placeHolder: 'https://reports.example.com',
        validateInput: value => /^(https:\/\/|http:\/\/(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$))/.test(value)
          ? undefined : 'Use an https address, or http on this machine.',
      });
      if (!server) return vscode.window.showErrorMessage('Name your RichView server (setting richview.server) to publish.');
      await settings.update('server', server, vscode.ConfigurationTarget.Global);
    }
    let token = await context.secrets.get('richview.token:' + server);
    if (!token) {
      token = await vscode.window.showInputBox({ prompt: 'RichView API token for ' + server, password: true });
      if (!token) return;
      await context.secrets.store('richview.token:' + server, token);
    }
    const publish = isNew => ask('richview/publish', { uri: document.uri.toString(), server, token, new: isNew });
    let result;
    try {
      result = await publish(false);
    } catch (error) {
      // Creating a second address for this file needs confirmation (FEA-SF-51).
      const finding = findingsOf(error).find(f => f.code === 'publish.new-slug');
      if (!finding) throw error;
      const choice = await vscode.window.showWarningMessage(
        `${finding.message} Create it on ${server}?`, { modal: true }, 'Create');
      if (choice !== 'Create') return;
      result = await publish(true);
    }
    const choice = await vscode.window.showInformationMessage('Published: ' + result.url, 'Copy link', 'Open');
    if (choice === 'Copy link') await vscode.env.clipboard.writeText(result.url);
    if (choice === 'Open') await vscode.env.openExternal(vscode.Uri.parse(result.url));
  });
  if (vscode.workspace.isTrusted) start();
  else context.subscriptions.push(vscode.workspace.onDidGrantWorkspaceTrust(start));
  context.subscriptions.push({
    dispose() { stopDependencies(); client?.stop(); client = undefined; }
  });
}
module.exports = { activate, deactivate() {} };
