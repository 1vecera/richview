// Host reason (ARC-03): the embedded source editor on the signed-in desk page. It mounts a text editor and
// calls the desk's JSON API; validation, rendering and every rule run on the server in Rust. It shows the
// two browser actions the lockdown allows (INT-26, FEA-BR-27): approve or reject a pending change, and an
// author's quick source edit with diagnostics at their source lines. The editor library is open (ARC-03);
// until one is chosen this is a plain text area with line numbers.
import { startDesk } from "./desk-client.ts";

export const deskHTML = `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>RichView Publish desk</title>
<style>body{font:15px/1.5 system-ui,sans-serif;margin:0 auto;max-width:960px;padding:24px}h1{font-size:20px}section{margin:24px 0}li{margin:6px 0}button{font:inherit;padding:4px 12px;margin-right:6px}
.src{display:grid;grid-template-columns:auto 1fr;border:1px solid #888;font:13px/1.5 ui-monospace,monospace}.nums{padding:8px;text-align:right;color:#777;background:#f3f3f3;white-space:pre;user-select:none}textarea{border:0;padding:8px;min-height:420px;font:inherit;white-space:pre;overflow:auto;resize:vertical}
.err{color:#a00}.ok{color:#060}</style>
<main><h1>RichView Publish desk</h1><p id="who"></p><section><h2>Changes awaiting review</h2><ul id="changes"></ul></section><section><h2>Your items</h2><ul id="items"></ul></section>
<section id="edit" hidden><h2 id="title"></h2><div class="src"><div class="nums" id="nums"></div><textarea id="text" spellcheck="false"></textarea></div><p><button id="save">Save as a pull request</button><span id="msg"></span></p><ul id="diag"></ul></section></main>
<script src="/_editor/app.js"></script></html>`;

// Wrangler compiles the function before this response is sent to the browser.
export const deskScript = `(${startDesk.toString()})();\n`;
