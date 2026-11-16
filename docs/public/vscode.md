---
title: Edit beside a live preview
description: Rich Markdown diagnostics, completion, formatting and export in VS Code.
styles: [scientific]
---

The extension is not on the Visual Studio Marketplace yet. It targets VS Code 1.90 or later and requires a trusted workspace.

## Install a VSIX

A downloadable VSIX is not available on this site yet. If you already have a package, replace the path below with its location:

```sh
code --install-extension /absolute/path/to/richview-vscode.vsix
```

<!-- repository-public:start -->
## Package from source

From the repository root, after the complete [source build](https://richview.ai/docs/install#build-from-source), package and install the extension:

```sh
npm --workspace richview-vscode run package
code --install-extension packages/vscode/richview-vscode-*.vsix
```

Use the [extension source](https://github.com/1vecera/richview/tree/dev/packages/vscode) when building or inspecting the package.
<!-- repository-public:end -->

## Open a report

Open a dedicated report folder and its `.rmd`. Run **RichView: Open Live Preview** from the command palette. The preview follows unsaved source edits; changes to referenced data and style files appear when those files are saved. **Export uses the saved source**, so save before sharing a result.

Outline, breadcrumbs and folding follow report headings and blocks; hover explains fields, and snippets insert valid starting points. Diagnostics identify malformed YAML, unsupported fields, missing files and invalid data fields. **Trigger Suggest** offers block keys, chart types, enumerated values and columns from inline or local data. It stays out of prose and freeform row contents.

## Format without rewriting prose

**Format Document** normalizes YAML only where its decoded values remain identical. It preserves prose, comments, unknown fences and exact scalar tokens; regions it cannot safely reindent stay unchanged. To enable ordinary VS Code format-on-save, use:

```json
{
  "[richview]": {
    "editor.defaultFormatter": "richview.richview-vscode",
    "editor.formatOnSave": true
  }
}
```

## Use the command palette

| Command | Result |
| --- | --- |
| **RichView: Open Live Preview** | Preview the current source buffer. |
| **RichView: Insert Block** | Pick a block and insert its starter source. |
| **RichView: Export to Rendered** | Generate the saved report into its `Rendered/` folder. |
| **RichView: Publish to Server** | Publish to the configured report server after a token prompt. |

The packaged editor service is the default. `richview.binary` selects another native editor service executable, not an arbitrary shell command. Configure `richview.server` for publishing. The masked token prompt stores the token per server in VS Code SecretStorage. Read [publishing](https://richview.ai/docs/publishing) before using it; a local preview does not publish a report.
