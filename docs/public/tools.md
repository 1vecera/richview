---
title: CLI and MCP reference
description: Commands, flags, tools and result fields generated from the implementation.
styles: [scientific]
---

Use [the CLI walkthrough](https://richview.ai/docs/cli) or [MCP setup](https://richview.ai/docs/mcp) for the working path. This page supplies the exact vocabulary the installed command exposes. Publication goes to a [separate report server](https://richview.ai/docs/publishing).

## CLI command and flag reference

<!-- richview:cli-reference -->

## MCP over standard input/output

Configure a stdio server with the absolute executable path and the argument `mcp`. Standard output carries newline-framed JSON-RPC replies. [MCP setup](https://richview.ai/docs/mcp) explains configuration and an example render request.

<!-- richview:mcp-reference -->

## Results and diagnostics

Report operations carry `ok`, `status`, `output` and `diagnostics`; MCP exposes that JSON in `structuredContent`. `status: clean` means no warnings or errors. `ok: true` means no errors, so read diagnostics even after a successful operation. `isError` describes whether an MCP tool could execute. Lines and columns start at 1; a diagnostic's `file` names the data or style file when applicable, otherwise its location is in the `.rmd`.

`learn` prints text. `serve`, `server` and `mcp` remain running until stopped. `asset` and `image` have their own result objects and transfer exit codes. [Diagnostics and repair](https://richview.ai/docs/recovery) explains recovery; [data and storage](https://richview.ai/docs/data) describes asset transfer results.

## Use the VS Code extension

The extension is not on the Visual Studio Marketplace yet. Read the [VS Code guide](https://richview.ai/docs/vscode) for its native service, trusted workspace, preview, completion, formatting, export and publishing settings.

## A render tool call

MCP clients construct this request from an ordinary instruction to render a saved report:

```json
{"name":"richview_render","arguments":{"inputPath":"/absolute/path/to/first-report/report.rmd"}}
```
