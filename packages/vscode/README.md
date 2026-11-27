# RichView for VS Code

Edit `.rmd` source (language `richview`, "Rich Markdown") with highlighting, live diagnostics, formatting, schema completions, an outline, folding, hover, snippets and a live preview. A small JavaScript module starts the native `richview-editor` as a language server (Language Server Protocol over stdio, with `vscode-languageclient`) and asks it for everything else; Rust owns authoring, schemas, rendering and diagnostics. The Problems panel shows exactly what `richview validate` returns for the text in the editor.

The extension runs only in trusted workspaces. `richview.binary` (machine scope) points at another editor binary; otherwise the packaged one runs. If another extension also claims `.rmd`, set `"files.associations": {"*.rmd": "richview"}`.

## Commands

- **RichView: Open Live Preview** opens a webview beside the editor. It renders the unsaved text, refreshes on every edit and on any change in the report folder except `Rendered/`, follows the active `.rmd` editor and shows ✓, ▲ or ✕ in its title. When a render fails, the last good render stays under a banner with the findings.
- **RichView: Insert Block** is one quick pick of every block and chart type, from the service's own catalogue. The `rv-*` snippets cover the same blocks.
- **RichView: Export to Rendered** writes what `richview generate` writes for the saved file; it refuses an unsaved document.
- **RichView: Publish to Server** needs `richview.server` (machine scope, no default) and saves the token for that server in VS Code secret storage. Nothing publishes implicitly.

## Building

`cargo run --release --locked -p richview-build` packages the current platform's native service, regenerates the snippets and the highlighted fence tags from core, and bundles the activation module with its language client into `dist/extension.js`. `npm --workspace richview-vscode run package` then creates the local VSIX; install the VSIX built for your operating system and architecture. Packaged font notices are in `font-licenses/`.

## Verification

`scripts/verify-editor.cts` runs the adapter against the real editor service over stdio through a VS Code API double. The service's own protocol, diagnostics and providers are tested in `crates/richview-editor/tests/lsp.rs`.
