// Host reason (ARC-03): exercise the installed VSIX inside a private real VS Code test host.
import type * as VSCode from 'vscode';
import type { Point, Rect } from './report-model.ts';
const assert: typeof import('node:assert/strict') = require('node:assert/strict');
const fs: typeof import('node:fs') = require('node:fs');
const path: typeof import('node:path') = require('node:path');
const vscode: typeof VSCode = require('vscode');
const {execFileSync}: typeof import('node:child_process') = require('node:child_process');
const {pathToFileURL}: typeof import('node:url') = require('node:url');

type Rows = Record<string, string | number | boolean | null>[];
type Sample = { font: string; bg: string; fill: string; content: string; data: Rows[] };
type Banner = { text: string; role: string | null };
type ModelStatus = { rows: Rows; pending: string[]; notices: number };
type StepPreview = ModelStatus & { vertices: Point[]; columns: string[] };
type HistogramPreview = ModelStatus & { rects: Rect[]; binning: { edges: number[]; measure: string } };
type BoxplotPreview = ModelStatus & { columns: string[]; orientation: string; box: Rect; outlier: { cx: number; cy: number } };
type IntervalPreview = ModelStatus & { orientation: string; ranges: { x: number; y: number; width: number }[]; estimates: number; readout: string };
type RuntimeResult<T = unknown> = { result?: { value?: T }; exceptionDetails?: Record<string, unknown> };
type BrowserContext = { id: number; auxData?: { isDefault?: boolean } };
type CdpMessage = { id?: number; result?: unknown; error?: { message: string }; method?: string; params?: { context?: BrowserContext; executionContextId?: number } };
exports.run = async () => {
  const artifact = process.env.RICHVIEW_VSCODE_ARTIFACT_DIR;
  assert.ok(artifact, 'Use the isolated VS Code test runner');
  const expectedVersion = process.env.RICHVIEW_TEST_VSIX_VERSION;
  assert.ok(expectedVersion, 'The isolated runner must provide the VS Code package manifest version');
  const extension = vscode.extensions.getExtension('richview.richview-vscode');
  assert.ok(extension);
  const manifest: { version: string } = extension.packageJSON;
  assert.equal(manifest.version, expectedVersion, 'The installed VSIX must match the checkout’s VS Code package manifest');
  assert.ok(extension.extensionPath.startsWith(process.env.RICHVIEW_TEST_EXTENSIONS!), 'Tests must exercise the VSIX installation');
  await extension.activate();
  const root = vscode.workspace.workspaceFolders![0].uri.fsPath;
  const file = vscode.Uri.file(path.join(root, 'report.rmd'));
  const original = fs.readFileSync(file.fsPath, 'utf8');
  const runWriter=(script: string,...args: string[])=>execFileSync('node',['-e',script,...args]);
  const writeDependency=(file: string,text: string)=>runWriter('require("node:fs").writeFileSync(process.argv[1],process.argv[2])',file,text);
  const document = await vscode.workspace.openTextDocument(file);
  assert.equal(document.languageId,'richview','The installed extension owns the Rich Markdown language');
  const editor = await vscode.window.showTextDocument(document,{viewColumn:vscode.ViewColumn.One});
  const wait = async (predicate: () => boolean | Promise<boolean>) => { for(let i=0;i<150;i++){if(await predicate()) return; await new Promise(r=>setTimeout(r,100));} throw new Error('VS Code behavior timed out'); };
  const replace = async (text: string) => {const edit=new vscode.WorkspaceEdit();edit.replace(file,new vscode.Range(document.positionAt(0),document.positionAt(document.getText().length)),text);assert.ok(await vscode.workspace.applyEdit(edit));};
  await wait(()=>vscode.languages.getDiagnostics(file).every(d=>d.severity!==vscode.DiagnosticSeverity.Error));
  await replace(original+'\n```chart\ntype: invalid\n```\n');
  await wait(()=>vscode.languages.getDiagnostics(file).some(d=>d.code==='block.schema'));
  const invalidLine=document.getText().split('\n').findIndex(s=>s==='type: invalid');
  assert.ok(vscode.languages.getDiagnostics(file).some(d=>d.range.start.line===invalidLine && d.range.end.character===4));
  await replace(original); await wait(()=>!vscode.languages.getDiagnostics(file).some(d=>d.severity===vscode.DiagnosticSeverity.Error));
  await replace(original.replace('title: Revenue', 'title:    Revenue'));
  const edits=await vscode.commands.executeCommand<VSCode.TextEdit[]>('vscode.executeFormatDocumentProvider',file,{tabSize:2,insertSpaces:true});
  assert.equal(edits.length,1);
  const edit=new vscode.WorkspaceEdit();edit.set(file,edits);assert.ok(await vscode.workspace.applyEdit(edit));
  assert.equal(document.getText(),original);
  assert.equal((await vscode.commands.executeCommand<VSCode.TextEdit[]>('vscode.executeFormatDocumentProvider',file,{tabSize:2,insertSpaces:true})||[]).length,0);
  // Normal optional format-on-save, exercised by saving a real document.
  await vscode.workspace.getConfiguration('editor',{uri:file,languageId:document.languageId}).update('defaultFormatter','richview.richview-vscode',vscode.ConfigurationTarget.Workspace);
  await vscode.workspace.getConfiguration('editor',{uri:file,languageId:document.languageId}).update('formatOnSave',true,vscode.ConfigurationTarget.Workspace);
  await replace(original.replace('title: Revenue','title:    Revenue'));assert.ok(await document.save());assert.equal(document.getText(),original);
  let line=document.getText().split('\n').findIndex(s=>s==='type: bar');
  let choices=await vscode.commands.executeCommand<VSCode.CompletionList>('vscode.executeCompletionItemProvider',file,new vscode.Position(line,7));
  assert.ok(choices.items.some(i=>i.label==='bar'));
  await replace('```chart\ntype: \n```\n');
  choices=await vscode.commands.executeCommand<VSCode.CompletionList>('vscode.executeCompletionItemProvider',file,new vscode.Position(1,6));
  assert.ok(choices.items.some(i=>i.label==='histogram'),'Histogram type completion');
  assert.ok(choices.items.some(i=>i.label==='boxplot'),'Boxplot type completion');
  assert.ok(choices.items.some(i=>i.label==='interval'),'Interval type completion');
  for(const channel of ['lower','upper']){
    await replace('```chart\ntype: interval\ndata: {inline: [{category: A, low: 1, high: 3}]}\nencoding:\n  '+channel+': \n```\n');
    choices=await vscode.commands.executeCommand<VSCode.CompletionList>('vscode.executeCompletionItemProvider',file,new vscode.Position(4,9));
    assert.ok(choices.items.some(item=>item.label==='low')&&choices.items.some(item=>item.label==='high'),'Interval bound fields come from the actual data');
  }
  // insertSnippet writes into the active editor: VS Code 1.136's agent host can open its chat view
  // and take focus, so show this document first and retry until the snippet has landed in it.
  await wait(async()=>{
    await replace('');
    await vscode.window.showTextDocument(document,{preview:false,preserveFocus:false});
    await vscode.commands.executeCommand('editor.action.insertSnippet',{langId:document.languageId,name:'interval'});
    return /type: interval/.test(document.getText());
  });
  assert.match(document.getText(),/type: interval/);
  assert.match(document.getText(),/lower: lower, upper: upper, y: estimate/);
  // Problems follows validate (FEA-SF-24); geometry errors belong to check and the preview banner.
  const sourceRanges: { token: string; range: VSCode.Range; message: string }[]=[];
  const expectSourceRange=async(token: string,message: string)=>{
    const offset=document.getText().indexOf(token);assert.ok(offset>=0);
    const expected=new vscode.Range(document.positionAt(offset),document.positionAt(offset+token.length));
    try { await wait(()=>vscode.languages.getDiagnostics(file).some(diagnostic=>diagnostic.code==='block.schema'&&diagnostic.severity===vscode.DiagnosticSeverity.Error&&diagnostic.message.includes(message)&&diagnostic.range.isEqual(expected))); }
    catch(error){fs.writeFileSync(path.join(artifact,'source-range-failure.json'),JSON.stringify({source:document.getText(),token,expected,diagnostics:vscode.languages.getDiagnostics(file)},null,2));throw error;}
    const issue=vscode.languages.getDiagnostics(file).find(diagnostic=>diagnostic.code==='block.schema'&&diagnostic.message.includes(message));
    assert.ok(issue);
    assert.equal(issue.severity,vscode.DiagnosticSeverity.Error);
    assert.ok(issue.range.isEqual(expected),`Actual source span for ${token}: ${JSON.stringify(issue.range)}`);
    assert.equal(document.getText(issue.range),token);
    sourceRanges.push({token,range:issue.range,message:issue.message});
  };
  await replace('```chart\ntype: interval\ntitle: Value types\ndata: {inline: [{category: A, lo: 3, hi: 2}]}\nencoding: {x: category, lower: 3, upper: hi}\n```\n');
  await expectSourceRange('lower','chart.encoding.lower must be a data column name, not 3');
  await replace('# 🦀 Prefix\r\n\r\n```chart\r\ntype: interval\r\ntitle: Value types\r\ndata: {inline: [{category: A, lo: 3, hi: 2}]}\r\nencoding: {x: "🦀 category", lower: 3, upper: hi}\r\n```\r\n');
  await expectSourceRange('lower','chart.encoding.lower must be a data column name, not 3');
  await replace('```chart\ntype: interval\ntitle: Value types\ndata: {csv: false}\nencoding: {x: category, lower: lo, upper: hi}\n```\n');
  await expectSourceRange('csv','chart.data.csv must be text');
  fs.writeFileSync(path.join(artifact,'source-ranges.json'),JSON.stringify(sourceRanges,null,2));
  const renderErrorSources=[
    {name:'inline',source:'```chart\ntype: interval\ntitle: Reversed bounds inline\ndata: {inline: [{category: A, lower: 3, upper: 2}]}\nencoding: {x: category, lower: lower, upper: upper}\n```\n'},
    {name:'unicode',source:'# 🦀 Prefix\r\n\r\n```chart\r\ntype: interval\r\ntitle: Reversed bounds unicode\r\ndata: {inline: [{category: "🦀 First", "lo/~.🦀": 5, hi: 4}]}\r\nencoding: {x: category, lower: "lo/~.🦀", upper: hi}\r\n```\r\n'},
    {name:'external',source:"```chart\ntype: interval\ntitle: Reversed bounds external\ndata: {csv: 'ranges.csv'}\nencoding: {x: category, lower: lo, upper: hi}\n```\n"},
  ];
  fs.writeFileSync(path.join(root,'ranges.csv'),'category,lo,hi\nFirst,5,4\n');
  await replace(original);
  await wait(()=>!vscode.languages.getDiagnostics(file).some(diagnostic=>diagnostic.severity===vscode.DiagnosticSeverity.Error));
  line=document.getText().split('\n').findIndex(s=>s.startsWith('encoding:'));
  choices=await vscode.commands.executeCommand<VSCode.CompletionList>('vscode.executeCompletionItemProvider',file,new vscode.Position(line,15));
  assert.ok(choices.items.some(i=>i.label==='month'));
  await replace('```chart\ntitle: 🦀\naxes:\n  x:\n    lo\n```\n');
  choices=await vscode.commands.executeCommand<VSCode.CompletionList>('vscode.executeCompletionItemProvider',file,new vscode.Position(1,9));
  assert.equal(choices.items.length,0);
  choices=await vscode.commands.executeCommand<VSCode.CompletionList>('vscode.executeCompletionItemProvider',file,new vscode.Position(4,6));
  assert.ok(choices.items.some(i=>i.label==='locale'),'Unicode input must leave the persistent editor service usable');
  await replace('```chart\nstyle:\n  curve: step\n```\n');
  choices=await vscode.commands.executeCommand<VSCode.CompletionList>('vscode.executeCompletionItemProvider',file,new vscode.Position(2,13));
  for(const curve of ['step','step_before','step_after']) assert.ok(choices.items.some(i=>i.label===curve),'Step curve completion: '+curve);
  await replace('```chart\ntype: histogram\nbinning:\n  measure: \n```\n');
  choices=await vscode.commands.executeCommand<VSCode.CompletionList>('vscode.executeCompletionItemProvider',file,new vscode.Position(3,11));
  for(const measure of ['count','proportion','density'])assert.ok(choices.items.some(i=>i.label===measure),'Histogram measure completion: '+measure);
  const navigationSource="---\r\ntitle: Metadata\r\n---\r\n# 🦀 **Revenue**\r\nIntro\r\n## Distribution\r\n```chart\r\ntitle: 'Wait 🦀'\r\ndata: [unfinished\r\n```\r\n\r\n# Next\r\nLast\r\n";
  const navigationDocument=await vscode.workspace.openTextDocument({language:document.languageId,content:navigationSource});
  const [outline,folds]=await Promise.all([
    vscode.commands.executeCommand<VSCode.DocumentSymbol[]>('vscode.executeDocumentSymbolProvider',navigationDocument.uri),
    vscode.commands.executeCommand<VSCode.FoldingRange[]>('vscode.executeFoldingRangeProvider',navigationDocument.uri)
  ]);
  assert.equal(navigationDocument.isUntitled,true,'Source navigation works before a report is saved');
  assert.equal(navigationDocument.getText(),navigationSource,'Navigation leaves unfinished source intact');
  assert.deepEqual(outline.map(item=>item.name),['🦀 Revenue','Next']);
  assert.equal(outline[0].kind,vscode.SymbolKind.String);
  assert.ok(outline[0].selectionRange.isEqual(new vscode.Range(3,2,3,14)),'Navigation uses UTF-16 source positions');
  const chartSymbol=outline[0].children[0].children[0];
  assert.equal(outline[0].children[0].name,'Distribution');
  assert.equal(chartSymbol.name,'Wait 🦀');
  assert.equal(chartSymbol.kind,vscode.SymbolKind.Object);
  assert.ok(outline[0].range.contains(chartSymbol.range));
  assert.ok(chartSymbol.range.contains(chartSymbol.selectionRange));
  assert.ok(folds.some(fold=>fold.start===0&&fold.end===2),'Frontmatter fold');
  assert.ok(folds.some(fold=>fold.start===6&&fold.end===9),'Partial rich-block content retains its fence fold');
  const navigationEdit=new vscode.WorkspaceEdit();
  navigationEdit.replace(navigationDocument.uri,new vscode.Range(7,0,7,navigationDocument.lineAt(7).text.length),'title: Updated distribution');
  assert.ok(await vscode.workspace.applyEdit(navigationEdit));
  const updatedOutline=await vscode.commands.executeCommand<VSCode.DocumentSymbol[]>('vscode.executeDocumentSymbolProvider',navigationDocument.uri);
  assert.equal(updatedOutline[0].children[0].children[0].name,'Updated distribution','Navigation follows the latest unsaved version');
  fs.writeFileSync(path.join(artifact,'navigation.json'),JSON.stringify({untitled:true,outline,folds,updatedOutline},null,2));
  // Fields also come from an existing source-relative CSV, without network resolution.
  fs.writeFileSync(path.join(root,'sales.csv'),'month,value\nJan,100\nFeb,140\n');
  await replace('```table\ndata: {csv: sales.csv}\ncolumns:\n  - key: mo\n```\n');
  choices=await vscode.commands.executeCommand<VSCode.CompletionList>('vscode.executeCompletionItemProvider',file,new vscode.Position(3,11));
  assert.ok(choices.items.some(i=>i.label==='month'));
  const {chromium}: typeof import('playwright')=require('playwright');
  const browser=await chromium.connectOverCDP('http://127.0.0.1:'+process.env.RICHVIEW_TEST_CDP_PORT);
  try {
  const workbench=browser.contexts().flatMap(context=>context.pages()).find(page=>page.url().includes('workbench'));
  assert.ok(workbench,'The isolated VS Code workbench has a CDP page');
  await replace(original);
  editor.selection=new vscode.Selection(document.positionAt(document.getText().length),document.positionAt(document.getText().length));
  const insertion=vscode.commands.executeCommand('richview.insertBlock');
  const picker=workbench.locator('.quick-input-widget');
  await picker.waitFor({state:'visible'});
  await picker.locator('input[type="text"]').fill('Table');
  await picker.getByText('Table',{exact:true}).click();
  await insertion;
  assert.ok(document.getText().endsWith('    align: right\n```\n'));
  await replace(original); await document.save();
  await vscode.commands.executeCommand('workbench.action.closeAuxiliaryBar');
  await vscode.commands.executeCommand('workbench.action.closeSidebar');
  const previewTimings: { name: string; responseMs?: number; interactiveMs: number }[]=[];
  let pendingPreviewTiming: { started: number; responseMs?: number } | undefined={started:Date.now()};
  await vscode.commands.executeCommand('richview.preview');
  pendingPreviewTiming.responseMs=Date.now()-pendingPreviewTiming.started;
  // Electron exposes VS Code's custom-scheme webview as an OOP iframe target.
  // Inspect its actual execution contexts through CDP, including the inner report frame.
  const screenshot=(name: string)=>workbench.screenshot({path:path.join(artifact,name)});
  const targets=await browser.newBrowserCDPSession();
  let target;
  for(let i=0;i<100&&!target;i++){
    target=(await targets.send('Target.getTargets')).targetInfos.find(t=>t.type==='iframe'&&t.url.startsWith('vscode-webview:'));
    if(!target) await new Promise(r=>setTimeout(r,100));
  }
  assert.ok(target,'Actual installed webview target');
  const attached=await targets.send('Target.attachToTarget',{targetId:target.targetId,flatten:false});
  const pending=new Map<number, { resolve(value: unknown): void; reject(reason: unknown): void }>();
  const contexts=new Map<number, BrowserContext>();const previewEvents: CdpMessage[]=[];let requestId=0;
  targets.on('Target.receivedMessageFromTarget',(event: { sessionId: string; message: string })=>{
    if(event.sessionId!==attached.sessionId)return;
    const message: CdpMessage=JSON.parse(event.message);
    if(message.id){const callback=pending.get(message.id);pending.delete(message.id);if(callback) message.error?callback.reject(new Error(message.error.message)):callback.resolve(message.result);}
    if(message.method==='Runtime.executionContextCreated')contexts.set(message.params!.context!.id,message.params!.context!);
    if(message.method==='Runtime.executionContextDestroyed')contexts.delete(message.params!.executionContextId!);
    if(message.method==='Runtime.executionContextsCleared')contexts.clear();
    if(message.method==='Runtime.exceptionThrown'||message.method==='Network.loadingFailed'||message.method==='Log.entryAdded')previewEvents.push(message);
  });
  // CDP evaluates the string in another world. Each call that reads its return value names that shape.
  const send=<Result = unknown,>(method: string,params: object={})=>new Promise<Result>((resolve,reject)=>{
    const id=++requestId;pending.set(id,{resolve: value => resolve(value as Result),reject});
    targets.send('Target.sendMessageToTarget',{sessionId:attached.sessionId,message:JSON.stringify({id,method,params})}).catch(reject);
  });
  await send('Runtime.enable');
  await send('Network.enable');
  await send('Log.enable');
  // In-page helpers over the report runtime: the chart's payload, its rows as objects, its geometry
  // model and the readout after Home on the focused frame.
  const previewHelpers=`
    const figure=()=>document.querySelector('figure[data-rv-chart]');
    const payloadOf=node=>JSON.parse(node.querySelector('script[data-rv-payload]').textContent);
    // Quantized rows are layout positions; shown carries the authored Table/CSV values.
    const rowsOf=payload=>payload.rows.map((row,rowIndex)=>Object.fromEntries(payload.columns.map((column,index)=>
      [column,payload.shown?.[column]?.[rowIndex] ?? row[index]])));
    const readoutAfterHome=async()=>{
      const frame=figure().querySelector('.rv-chart__frame');
      frame.focus({preventScroll:true});frame.dispatchEvent(new KeyboardEvent('keydown',{key:'Home',bubbles:true,cancelable:true}));
      await window.__richview.settled();
      await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
      const readout=figure().querySelector('.rv-chart__readout');return readout&&!readout.hidden?readout.textContent:null;
    };
    const marks=kind=>window.__richview.charts()[0].geometry.panels.flatMap(panel=>panel.items).filter(item=>item.kind===kind&&item.role==='mark');
    const notices=()=>[...document.querySelectorAll('.rv-chart__notice')].filter(notice=>!notice.hidden&&getComputedStyle(notice).display!=='none').length;
  `;
  const appearance: (Sample & { name: string })[]=[];
  const expression=`(async()=>{
    ${previewHelpers}
    const body=document.body;
    if(!window.__richview)return null;
    await window.__richview.settled();
    const bar=window.__richview.charts().find(model=>model.geometry.kind==='bar');
    const color=bar?.geometry.series[0]?.color;
    if(!color)return null;
    await document.fonts.ready;
    const [r,g,b]=color.slice(0,3).map(value=>Math.round(value*255));
    const fill='rgb('+r+', '+g+', '+b+')';
    const report=body.querySelector('.rv-report').cloneNode(true);
    report.querySelectorAll('.rv-chart, script, footer, .rv-report-card').forEach(n=>n.remove());
    // The page colour, read from the style: a page background (STY-74) leaves the body transparent.
    return {font:getComputedStyle(body.querySelector('.rv-report__body')).fontFamily,
      bg:getComputedStyle(document.documentElement).getPropertyValue('--rv-bg').trim(),fill,
      content:report.textContent.replace(/\\s+/g,' ').trim(),
      data:[...body.querySelectorAll('figure[data-rv-chart]')].map(node=>rowsOf(payloadOf(node)))};
  })()`;
  const capture=async(name: string,expectedFill?: string,accept: (sample: Sample) => boolean=()=>true)=>{
    let lastValue;const exceptions=[];
    for(let attempt=0;attempt<200;attempt++){
      for(const context of contexts.values()){
        if(!context.auxData?.isDefault)continue;
        try {
          const result=await send<RuntimeResult<Sample>>('Runtime.evaluate',{expression,contextId:context.id,awaitPromise:true,returnByValue:true});
          if(result.exceptionDetails)exceptions.push({context:context.id,...result.exceptionDetails});
          const value=result.result?.value;
          if(value)lastValue=value;
          if(!value || (expectedFill && value.fill!==expectedFill) || !accept(value))continue;
          if(pendingPreviewTiming){
            previewTimings.push({name,responseMs:pendingPreviewTiming.responseMs,interactiveMs:Date.now()-pendingPreviewTiming.started});
            fs.writeFileSync(path.join(artifact,'preview-timings.json'),JSON.stringify(previewTimings,null,2));
            pendingPreviewTiming=undefined;
          }
          appearance.push({name,...value});
          await send('Runtime.evaluate',{expression:"document.querySelector('.rv-chart').scrollIntoView({block:'center'})",contextId:context.id});
          editor.revealRange(new vscode.Range(8,0,18,0),vscode.TextEditorRevealType.AtTop);
          await send('Runtime.evaluate',{expression:'(async()=>{await window.__richview.settled();await new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(done)))})()',contextId:context.id,awaitPromise:true});
          await screenshot('preview-'+name+'.png');
          return value;
        } catch(error){if(attempt===199)throw error;}
      }
      await new Promise(r=>setTimeout(r,100));
    }
    fs.writeFileSync(path.join(artifact,'cdp-contexts.json'),JSON.stringify([...contexts.values()],null,2));
    const states=[];
    for(const context of contexts.values()){
      if(!context.auxData?.isDefault)continue;
      try{
        const state=await send<RuntimeResult>('Runtime.evaluate',{expression:"({url:location.href,status:document.documentElement.dataset.rvRuntime,hook:typeof window.__richview,keys:Object.keys(window.__richview??{}),kinds:window.__richview?.charts?.().map(model=>model.geometry.kind),canvas:document.querySelectorAll('canvas.rv-chart__canvas').length,banner:document.querySelector('#richview-banner')?.textContent,scripts:[...document.scripts].map(script=>({src:script.src,type:script.type})),text:document.body?.textContent.slice(0,300)})",contextId:context.id,returnByValue:true});
        states.push({context:context.id,...state});
      }catch{/* A replaced context has no current document to inspect. */}
    }
    fs.writeFileSync(path.join(artifact,'preview-failure.json'),JSON.stringify({name,expectedFill,lastValue,exceptions,states,events:previewEvents},null,2));
    throw new Error('Installed VSIX preview did not reach expected state: '+name);
  };
  const refresh=async (text: string)=>{
    const started=Date.now();
    await replace(text);await document.save();
    await vscode.window.showTextDocument(document,{viewColumn:vscode.ViewColumn.One});
    const requested=Date.now();await vscode.commands.executeCommand('richview.preview');
    pendingPreviewTiming={started,responseMs:Date.now()-requested};
  };
  const custom=await capture('source-local');assert.equal(custom.fill,'rgb(165, 42, 42)');
  assert.deepEqual(custom.data,[[{month:'Jan',value:100},{month:'Feb',value:140}]],'The installed preview retains the authored observations');
  const renderErrors=[];
  for(const {name,source} of renderErrorSources){
    await refresh(source);
    let banner: Banner | undefined;
    await wait(async()=>{
      for(const context of contexts.values()){
        if(!context.auxData?.isDefault)continue;
        try{
          const result=await send<RuntimeResult<Banner>>('Runtime.evaluate',{expression:`(()=>{const banner=document.querySelector('#richview-banner');return banner&&document.body.textContent.includes(${JSON.stringify('Reversed bounds '+name)})?{text:banner.textContent,role:banner.getAttribute('role')}:null})()`,contextId:context.id,returnByValue:true});
          if(result.result?.value?.text?.includes('chart.source-error')&&result.result.value.text.includes('lower bound')){banner=result.result.value;return true;}
        }catch{/* A replaced webview context belongs to the previous source. */}
      }
      return false;
    });
    assert.equal(banner!.role,'alert');
    assert.ok(vscode.window.tabGroups.all.flatMap(group=>group.tabs).some(tab=>tab.label==='RichView Preview ✕'),'A render error marks the actual preview tab as an error');
    renderErrors.push({name,...banner,severity:'error',readyMs:Date.now()-pendingPreviewTiming!.started});
    pendingPreviewTiming=undefined;
  }
  fs.writeFileSync(path.join(artifact,'render-errors.json'),JSON.stringify(renderErrors,null,2));
  await refresh(original);
  const recovered=await capture('recovered-source-local',custom.fill);
  assert.deepEqual(recovered.data,custom.data,'Repairing a render error restores the actual source data');
  const styleFile=vscode.Uri.file(path.join(root,'report.rstyle.yaml'));
  const styleDoc=await vscode.workspace.openTextDocument(styleFile);
  // The fixture's brand colour reaches the first chart fill unchanged (rgb(165, 42, 42) above); the probe changes it.
  const nextStyle=styleDoc.getText().replaceAll("brand: '#A52A2A'","brand: '#166534'");
  assert.notEqual(nextStyle,styleDoc.getText(),'The saved-style probe must change the source fixture');
  const styleEdit=new vscode.WorkspaceEdit();styleEdit.replace(styleFile,new vscode.Range(styleDoc.positionAt(0),styleDoc.positionAt(styleDoc.getText().length)),nextStyle);
  assert.ok(await vscode.workspace.applyEdit(styleEdit));await styleDoc.save();
  const edited=await capture('yaml-save','rgb(22, 101, 52)');assert.equal(edited.fill,'rgb(22, 101, 52)');
  // A separate process changes a dependency without saving through VS Code.
  writeDependency(styleFile.fsPath,nextStyle.replaceAll('#166534','#553c9a'));
  const external=await capture('external-yaml-write','rgb(85, 60, 154)');
  assert.equal(external.fill,'rgb(85, 60, 154)');
  writeDependency(styleFile.fsPath,nextStyle);
  await capture('external-yaml-restored',edited.fill);
  const canonical=[];
  for(const name of ['scientific','sleek','brutalist'] as const){
    await refresh(original.replace('styles: [scientific, report.rstyle.yaml]',`styles: [${name}]`));
    canonical.push(await capture(name,{scientific:'rgb(26, 109, 172)',sleek:'rgb(25, 91, 255)',brutalist:'rgb(18, 74, 140)'}[name]));
  }
  assert.equal(new Set(canonical.map(v=>v.font)).size,3);assert.equal(new Set(canonical.map(v=>v.bg)).size,3);assert.equal(new Set(canonical.map(v=>v.fill)).size,3);
  await refresh(original);const back=await capture('switch-back',edited.fill);assert.equal(back.fill,edited.fill);assert.equal(back.font,edited.font);
  await refresh(original.replace('styles: [scientific, report.rstyle.yaml]\n',''));
  fs.unlinkSync(styleFile.fsPath);const noYaml=await capture('no-yaml',canonical[1].fill);assert.equal(noYaml.fill,canonical[1].fill);assert.equal(noYaml.font,canonical[1].font);
  for(const value of appearance){assert.equal(value.content,custom.content);assert.deepEqual(value.data,custom.data);}
  await vscode.window.showTextDocument(document,{viewColumn:vscode.ViewColumn.One});
  const exported=path.join(root,'Rendered/report.html');
  const exportStarted=Date.now(),exporting=vscode.commands.executeCommand('richview.export');
  await workbench.getByText('Exported: '+exported,{exact:true}).waitFor();
  const exportMs=Date.now()-exportStarted;
  // The real command waits for its notification to be dismissed; Reveal opens an OS window.
  await vscode.commands.executeCommand('notifications.clearAll');await exporting;
  const html=fs.readFileSync(exported,'utf8');assert.ok(html.includes('140'));assert.ok(!html.includes('vscode-resource'));
  const offline=await chromium.launch({executablePath:process.env.RICHVIEW_VSCODE_CHROMIUM,headless:true,args:['--no-sandbox']});
  try {
  const exportedPage=await offline.newPage();const requests: string[]=[];
  await exportedPage.route('**/*',route=>{if(/^(file|data|blob):/.test(route.request().url()))return route.continue();requests.push(route.request().url());return route.abort();});
  await exportedPage.goto(pathToFileURL(exported).href);
  await exportedPage.waitForFunction(()=>window.__richview);await exportedPage.evaluate(()=>window.__richview!.settled());
  const exportedFrame=exportedPage.locator('figure[data-rv-chart] .rv-chart__frame');
  await exportedFrame.focus();await exportedPage.keyboard.press('Home');
  const exportedReadout=exportedPage.locator('.rv-chart__readout:not([hidden])');
  await exportedReadout.waitFor();assert.match(await exportedReadout.innerText(),/100/);assert.deepEqual(requests,[]);
  await exportedPage.screenshot({path:path.join(artifact,'vscode-export-offline.png'),fullPage:true});
  } finally { await offline.close(); }
  await vscode.commands.executeCommand('richview.preview');await capture('after-export',noYaml.fill);
  await screenshot('vscode-authoring.png');
  // External data is not open in VS Code: filesystem events must refresh source values,
  // recover from a missing dependency and leave generated output outside the watch scope.
  const csvPath=path.join(root,'sales.csv');
  const csvSource=original.replace(/data:\n  inline:\n    - month: Jan\n      value: 100\n    - month: Feb\n      value: 140/,'data: {csv: sales.csv}').replace('encoding:','readout_digits: 6\nencoding:');
  assert.notEqual(csvSource,original);
  fs.writeFileSync(styleFile.fsPath,nextStyle);
  fs.writeFileSync(csvPath,'month,value\nJan,100\nFeb,140\n');
  await refresh(csvSource);
  const hasFirst=(value: number)=>(sample: Sample)=>sample.data[0]?.[0]?.value===value;
  const csv=[];
  csv.push(await capture('external-csv-initial',edited.fill,hasFirst(100)));
  writeDependency(csvPath,'month,value\nJan,321\nFeb,140\n');
  csv.push(await capture('external-csv-write',edited.fill,hasFirst(321)));
  runWriter('const fs=require("node:fs"),file=process.argv[1];for(let i=0;i<20;i++){fs.writeFileSync(file+".tmp","month,value\\nJan,"+(900+i)+"\\nFeb,140\\n");fs.renameSync(file+".tmp",file)}',csvPath);
  csv.push(await capture('external-csv-atomic-latest',edited.fill,hasFirst(919)));
  // FEA-CH-37 limits every reader surface to the authored precision, including exact CSV inputs.
  for(const [name,value,shown,display] of [['decimal','1.234567890123456789',1.23457,'1.23457'],['integer','9007199254740993',9007200000000000,'9.0072 × 10¹⁵']] as const){
    writeDependency(csvPath,`month,value\nJan,${value}\nFeb,140\n`);
    csv.push(await capture(`external-csv-authored-precision-${name}`,edited.fill,hasFirst(shown)));
    let readout: string | undefined;
    for(let attempt=0;attempt<100&&!readout;attempt++){
      for(const context of contexts.values()){
        if(!context.auxData?.isDefault)continue;
        try{
          const result=await send<RuntimeResult<string>>('Runtime.evaluate',{expression:`(async()=>{
            ${previewHelpers}
            if(!figure()||!window.__richview||rowsOf(payloadOf(figure()))[0]?.value!==${JSON.stringify(shown)})return null;
            await window.__richview.settled();
            return readoutAfterHome();
          })()`,contextId:context.id,awaitPromise:true,returnByValue:true});
          if(result.result?.value?.includes(display))readout=result.result.value;
        }catch{}
      }
      if(!readout)await new Promise(resolve=>setTimeout(resolve,100));
    }
    assert.ok(readout,`External CSV ${name} reaches the actual keyboard readout at six authored digits`);
    assert.ok(!readout.includes(value),'The readout does not expose precision beyond the authored digits');
  }
  runWriter('require("node:fs").unlinkSync(process.argv[1])',csvPath);
  await wait(()=>vscode.languages.getDiagnostics(file).some(d=>d.severity===vscode.DiagnosticSeverity.Error));
  writeDependency(csvPath,'month,value\nJan,100\nFeb,140\n');
  csv.push(await capture('external-csv-recreated',edited.fill,hasFirst(100)));
  await wait(()=>!vscode.languages.getDiagnostics(file).some(d=>d.severity===vscode.DiagnosticSeverity.Error));
  const marker='retained-after-generated-output';
  let marked;
  for(const context of contexts.values()){
    if(!context.auxData?.isDefault)continue;
    const result=await send<RuntimeResult<string>>('Runtime.evaluate',{expression:"document.querySelector('.rv-chart') ? (window.__watchMarker='"+marker+"') : null",contextId:context.id,returnByValue:true});
    if(result.result?.value===marker)marked=context.id;
  }
  assert.ok(marked);
  fs.mkdirSync(path.join(root,'Rendered'),{recursive:true});
  writeDependency(path.join(root,'Rendered','watcher-output.json'),'{"generated":true}');
  await new Promise(r=>setTimeout(r,700));
  const retained=await send<RuntimeResult<string>>('Runtime.evaluate',{expression:'window.__watchMarker',contextId:marked,returnByValue:true});
  assert.equal(retained.result?.value,marker,'Generated output must not replace the preview');
  fs.writeFileSync(path.join(artifact,'external-dependencies.json'),JSON.stringify({csv,generated_output_preserves_preview:true},null,2));
  const stepRows=[{x:0,value:2},{x:5,value:-3},{x:10,value:7}];
  await refresh('```chart\n'+JSON.stringify({type:'area',title:'Held observations',data:{inline:stepRows},encoding:{x:'x',y:'value'},style:{curve:'step_after'}})+'\n```\n');
  let stepPreview: StepPreview | undefined;
  for(let attempt=0;attempt<100&&!stepPreview;attempt++) {
    for(const context of contexts.values()) {
      if(!context.auxData?.isDefault)continue;
      try {
        const result=await send<RuntimeResult<StepPreview>>('Runtime.evaluate',{expression:`(async()=>{
          ${previewHelpers}
          if(!figure()||!window.__richview)return null;
          const payload=payloadOf(figure());
          if(payload.block.style?.curve!=='step_after')return null;
          await window.__richview.settled();
          await document.fonts.ready;
          const [line]=marks('line');
          return line?{vertices:line.path.cmds.map(command=>command.p),columns:payload.columns,rows:rowsOf(payload),pending:window.__richview.pending(),notices:notices()}:null;
        })()`,contextId:context.id,returnByValue:true,awaitPromise:true});
        if(result.result?.value) { stepPreview=result.result.value; break; }
      } catch { /* A replaced webview context is no longer available. */ }
    }
    if(!stepPreview)await new Promise(resolve=>setTimeout(resolve,100));
  }
  assert.ok(stepPreview,'The installed preview renders the new source curve');
  const {vertices}=stepPreview;
  assert.equal(vertices.length,5);
  for(let i=1;i<vertices.length;i++)assert.ok(vertices[i][0]===vertices[i-1][0]||vertices[i][1]===vertices[i-1][1],'Held values stay orthogonal in the actual webview');
  assert.deepEqual(stepPreview.rows,stepRows.map(row=>Object.fromEntries(stepPreview.columns.map(column=>[column,row[column as keyof typeof row]]))));
  assert.deepEqual(stepPreview.pending,[],'The preview must finish rendering before capture');
  assert.equal(stepPreview.notices,0,'No placeholder notice remains over the preview');
  fs.writeFileSync(path.join(artifact,'step-preview.json'),JSON.stringify(stepPreview,null,2));
  await screenshot('step-preview.png');
  const histogramRows=[{value:0},{value:0.5},{value:1},{value:3}];
  const geometryFailures: { name: string; message: string }[]=[];
  const checkGeometry=(name: string,check: () => void)=>{
    try { check(); } catch(error) { geometryFailures.push({name,message:error instanceof Error ? error.message : String(error)}); }
  };
  await refresh('```chart\n'+JSON.stringify({type:'histogram',title:'Equal mass across unequal intervals',data:{inline:histogramRows},encoding:{x:'value'},binning:{edges:[0,1,3],measure:'density'}})+'\n```\n');
  let histogramPreview: HistogramPreview | undefined;
  for(let attempt=0;attempt<100&&!histogramPreview;attempt++) {
    for(const context of contexts.values()) {
      if(!context.auxData?.isDefault)continue;
      try {
        const result=await send<RuntimeResult<HistogramPreview>>('Runtime.evaluate',{expression:`(async()=>{
          ${previewHelpers}
          if(!figure()||!window.__richview)return null;
          const payload=payloadOf(figure());
          if(payload.block.type!=='histogram')return null;
          await window.__richview.settled();await document.fonts.ready;
          const rects=marks('rect').map(item=>item.rect);
          return rects.length?{rects,rows:rowsOf(payload),binning:payload.block.binning,pending:window.__richview.pending(),notices:notices()}:null;
        })()`,contextId:context.id,returnByValue:true,awaitPromise:true});
        if(result.result?.value){histogramPreview=result.result.value;break;}
      } catch { /* A replaced webview context is no longer available. */ }
    }
    if(!histogramPreview)await new Promise(resolve=>setTimeout(resolve,100));
  }
  fs.writeFileSync(path.join(artifact,'histogram-preview.json'),JSON.stringify(histogramPreview,null,2));
  assert.ok(histogramPreview,'The installed preview renders a histogram');
  assert.deepEqual(histogramPreview.rows,histogramRows);
  assert.deepEqual(histogramPreview.binning.edges,[0,1,3]);
  assert.equal(histogramPreview.binning.measure,'density');
  assert.equal(histogramPreview.rects.length,2);
  const [firstBin,secondBin]=histogramPreview.rects;
  checkGeometry('histogram-widths',()=>assert.ok(Math.abs(firstBin.w*2-secondBin.w)<.2,'Unequal intervals retain their widths in the preview'));
  checkGeometry('histogram-area',()=>assert.ok(Math.abs(firstBin.w*firstBin.h-secondBin.w*secondBin.h)<firstBin.w*firstBin.h*.005,'Density gives equal probability equal rectangle area'));
  assert.deepEqual(histogramPreview.pending,[]);assert.equal(histogramPreview.notices,0);
  await screenshot('histogram-preview.png');
  const boxplotRows=[0,0,0,0,100].map((value,index)=>({sample:'Observed',value,record:`Original ${index}`}));
  await refresh('```chart\n'+JSON.stringify({type:'boxplot',title:'One original observation lies beyond the whiskers',data:{inline:boxplotRows},encoding:{x:'sample',y:'value'},style:{orientation:'horizontal'}})+'\n```\n');
  let boxplotPreview: BoxplotPreview | undefined;
  for(let attempt=0;attempt<100&&!boxplotPreview;attempt++) {
    for(const context of contexts.values()) {
      if(!context.auxData?.isDefault)continue;
      try {
        const result=await send<RuntimeResult<BoxplotPreview>>('Runtime.evaluate',{expression:`(async()=>{
          ${previewHelpers}
          if(!figure()||!window.__richview)return null;
          const payload=payloadOf(figure());
          if(payload.block.type!=='boxplot')return null;
          await window.__richview.settled();await document.fonts.ready;
          const [box]=marks('rect'),[outlier]=marks('point');
          if(!box||!outlier)return null;
          return {columns:payload.columns,rows:rowsOf(payload),orientation:payload.block.style?.orientation,box:box.rect,outlier:{cx:outlier.center[0],cy:outlier.center[1]},pending:window.__richview.pending(),notices:notices()};
        })()`,contextId:context.id,returnByValue:true,awaitPromise:true});
        if(result.result?.value){boxplotPreview=result.result.value;break;}
      } catch { /* A replaced webview context is no longer available. */ }
    }
    if(!boxplotPreview)await new Promise(resolve=>setTimeout(resolve,100));
  }
  assert.ok(boxplotPreview,'The installed preview renders a box plot and original outlier');
  assert.deepEqual(boxplotPreview.rows,boxplotRows.map(row=>Object.fromEntries(boxplotPreview.columns.map(column=>[column,row[column as keyof typeof row]]))));assert.equal(boxplotPreview.orientation,'horizontal');
  assert.equal(boxplotPreview.box.w,0,'Repeated observations keep their actual zero IQR');
  assert.ok(boxplotPreview.outlier.cx>boxplotPreview.box.x,'The observed outlier remains beyond the zero-IQR box');
  assert.deepEqual(boxplotPreview.pending,[]);assert.equal(boxplotPreview.notices,0);
  fs.writeFileSync(path.join(artifact,'boxplot-preview.json'),JSON.stringify(boxplotPreview,null,2));
  await screenshot('boxplot-preview.png');
  const intervalRows=[{category:'First',lo:'1.234567890123456789',hi:'2.345678901234567891',estimate:'1.9'},{category:'Second',lo:'3.00',hi:'4.000',estimate:null}];
  await refresh('```chart\n'+JSON.stringify({type:'interval',title:'Authored bounds retain their chosen precision',readout_digits:6,data:{inline:intervalRows},encoding:{x:'category',lower:'lo',upper:'hi',y:'estimate'},style:{orientation:'horizontal'}})+'\n```\n');
  let intervalPreview: IntervalPreview | undefined;
  for(let attempt=0;attempt<100&&!intervalPreview;attempt++) {
    for(const context of contexts.values()) {
      if(!context.auxData?.isDefault)continue;
      try {
        const result=await send<RuntimeResult<IntervalPreview>>('Runtime.evaluate',{expression:`(async()=>{
          ${previewHelpers}
          if(!figure()||!window.__richview)return null;
          const payload=payloadOf(figure());
          if(payload.block.type!=='interval')return null;
          await window.__richview.settled();await document.fonts.ready;
          // A range is the mark line spanning the bounds: along x when horizontal, so its ends share y.
          const ranges=marks('line').map(item=>item.path.cmds.map(command=>command.p)).filter(([from,to])=>from[1]===to[1]&&from[0]!==to[0]).map(([from,to])=>({x:Math.min(from[0],to[0]),y:from[1],width:Math.abs(to[0]-from[0])}));
          if(ranges.length!==2)return null;
          const readout=await readoutAfterHome();
          return {rows:rowsOf(payload),orientation:payload.block.style?.orientation,ranges,estimates:marks('point').length,readout,pending:window.__richview.pending(),notices:notices()};
        })()`,contextId:context.id,returnByValue:true,awaitPromise:true});
        if(result.result?.value?.readout?.includes('1.23457')){intervalPreview=result.result.value;break;}
      } catch { /* A replaced webview context is no longer available. */ }
    }
    if(!intervalPreview)await new Promise(resolve=>setTimeout(resolve,100));
  }
  assert.ok(intervalPreview,'The installed preview renders authored interval bounds at the chosen precision');
  assert.deepEqual(intervalPreview.rows,[{category:'First',lo:1.23457,hi:2.34568,estimate:1.9},{category:'Second',lo:3,hi:4,estimate:null}]);assert.equal(intervalPreview.orientation,'horizontal');
  assert.equal(intervalPreview.estimates,1,'A missing estimate does not acquire a point');
  assert.ok(intervalPreview.ranges.every(range=>Object.values(range).every(Number.isFinite)&&range.width>0));
  assert.ok(intervalPreview.readout.includes('Lower')&&intervalPreview.readout.includes('Upper')&&intervalPreview.readout.includes('2.34568'));
  assert.ok(!intervalPreview.readout.includes('1.234567890123456789')&&!intervalPreview.readout.includes('2.345678901234567891'));
  assert.deepEqual(intervalPreview.pending,[]);assert.equal(intervalPreview.notices,0);
  fs.writeFileSync(path.join(artifact,'interval-preview.json'),JSON.stringify(intervalPreview,null,2));
  await screenshot('interval-preview.png');
  fs.writeFileSync(path.join(artifact,'geometry-failures.json'),JSON.stringify(geometryFailures,null,2));
  assert.deepEqual(geometryFailures,[],'Every installed-preview geometry bound must pass');
  const evidence={host:'Installed VSIX in downloaded VS Code with private profile/extensions/workspace',platform:process.platform,architecture:process.arch,vscode:vscode.version,extension:extension.packageJSON.version,extensionPath:extension.extensionPath,checks:['activation','automatic lint without preview','actual schema-key range','diagnostics clear after repair','Format Document preserves source and is idempotent','normal optional format-on-save','chart-type completion','step curve completion','histogram type and measure completion','boxplot type completion','interval type and bound-field completion','interval snippet insertion','validation errors clear after repair','exact validation-key ranges including Unicode UTF-16 columns','render-only interval Errors appear in the preview banner for inline, Unicode and external inputs','inline field completion','Unicode completion preserves the service and nested-key suggestions','local CSV field completion','hierarchical outline on unfinished untitled source with exact UTF-16 ranges','frontmatter and rich-fence folding and latest unsaved navigation','table insertion','preview mounts an actual chart','step curve renders in the installed preview','histogram density renders with exact intervals and original rows','boxplot observed quartiles and original outlier render horizontally','interval authored bounds and optional estimates retain the chosen preview precision','computed canonical typography/background/chart palette changes','local YAML save refreshes preview','external YAML writes refresh preview','external CSV changes and atomic replacements refresh exact values','external CSV decimals and large integers retain authored precision in keyboard readouts','deleted dependency diagnostics recover after recreation','generated output does not reload preview','switch-back and missing YAML work','content and data invariant through all style switches','portable export inside Rendered'],appearance,preview_timings_ms:previewTimings,export_ms:exportMs,screenshot:'vscode-authoring.png',limits:'Current native host only; other hosts and Marketplace not exercised'};
  fs.writeFileSync(path.join(artifact,'vscode.json'),JSON.stringify(evidence,null,2));
  console.log(JSON.stringify(evidence,null,2));
  } finally { await browser.close(); }
};
