---
title: Connect your agent through MCP
description: Copy the configuration for your client, then ask for a render.
styles: [scientific]
---

Install the native command first. Find its absolute executable path with `command -v richview`, then replace `/absolute/path/to/richview` below. A client starts a local stdio process with the argument `mcp`; it needs no network server.

## Claude Code

Run this from your report project. Project scope writes a local `.mcp.json` file; approve it when Claude Code next opens the project.

```sh
claude mcp add --scope project richview -- /absolute/path/to/richview mcp
```

See [Claude Code's MCP setup](https://code.claude.com/docs/en/mcp).

## Codex

Add this to `~/.codex/config.toml`, then start a new Codex session:

```text config
[mcp_servers.richview]
command = "/absolute/path/to/richview"
args = ["mcp"]
```

See [Codex MCP configuration](https://developers.openai.com/codex/mcp/).

## Cursor

Create `.cursor/mcp.json` in your report project, then enable the server in Cursor's MCP settings:

```json
{
  "mcpServers": {
    "richview": {
      "type": "stdio",
      "command": "/absolute/path/to/richview",
      "args": ["mcp"]
    }
  }
}
```

See [Cursor's MCP configuration](https://cursor.com/docs/mcp).

## Ask for a render

Ask: “Render first-report/report.rmd with RichView and fix what it reports.” Use the client's file tools to create and edit source; RichView validates, checks and renders the saved report. For a PDF, ask for PDF output and A4 or Letter paper. The server process needs a local Chromium-family browser.

Report tools return the CLI result in `structuredContent`. `status: clean` means no errors or warnings. `ok: true` can still include warnings, so read the diagnostics. The [tool reference](https://richview.ai/docs/tools#mcp-over-standard-input-output) defines every argument and shows the raw render request.

The server exposes the format at `richview://spec/rmd` and the starter at `richview://examples/hello-world`. Store and photo credentials belong in the process environment. [Publishing](https://richview.ai/docs/publishing) is a separate, explicit operation.
