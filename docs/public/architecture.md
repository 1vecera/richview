---
title: How RichView becomes a report
description: The format, pure engines, native hosts, WebAssembly runtime and performance tradeoffs.
styles: [scientific]
---

RichView is a presentation layer for your agent. The agent writes Rich Markdown and local resources; a shared Rust renderer turns them into a report. You keep the editable source and observations alongside the result.

## One source through several hosts

```text
Rich Markdown + data + ordered styles
                ↓
        parse and validate
                ↓
      chart and report engines
                ↓
      HTML + SVG + chart payloads
                ↓
 browser runtime: interaction and controls
```

The chart engine owns scales, statistics, geometry and interaction policy. The report core owns Markdown, schemas, style resolution and report rendering. Both are pure: no filesystem, network, process, environment or clock access. Native and browser hosts supply resolved inputs.

The CLI, report server, MCP service and VS Code editor use the same operations. The browser runs Rust compiled to WebAssembly. Its small JavaScript loaders bridge the browser host; report source cannot provide executable code. The browser runtime adds chart readouts, controls, tables, expansion and motion.

## A report can travel

Native HTML export embeds the runtime, styles, resolved data, fonts and local media. Linked local documents are copied beside it, so share the output folder when those downloads matter. Credited Unsplash photos and external embeds retain their remote URLs. Keep the original source and resources to edit the report later.

The report server is separate from static site hosting. It stores immutable versions, source and resources; authentication and a separate report origin govern access. It can render stored source for comparison without rewriting the original artifact.

## Large inputs keep exact observations

Temporal charts retain observations separately from display geometry. Calendar arithmetic, transformations, aggregation and M4 reduction use the same engine across hosts. Reducing plotted positions does not replace source rows or exact readout values. Native font-bound estimates and browser glyph measurements are distinct checks.

The browser source editor keeps complete UTF-8 text in Rust. Large sources use navigable editing windows; rendering, copy and downloads use the complete source. Window-local undo resets on navigation or full-source replacement. VS Code owns its ordinary editor history.

## What performance measurements mean

Rendering, input latency, memory and payload sizes are separate workloads. A small inline bar chart, a long fifteen-minute series and a complete offline HTML file have different costs. Fonts and embedded runtime count toward the file you share; gzip transfer size is a different number.

The project's [performance notes](https://github.com/1vecera/richview/blob/dev/docs/PERFORMANCE.md) define reproducible fixtures and measurements. Their budgets are engineering targets, not universal speed promises. A measurement belongs to its build, workload, hardware and browser. The [architecture notes](https://github.com/1vecera/richview/blob/dev/docs/RUST-ARCHITECTURE.md) explain the module boundaries in more depth.

## Interfaces for integration

The native CLI, JSON HTTP/MCP operations and Rust APIs are the integration surfaces. The WebAssembly bindings serve RichView's own runtimes; no public npm package ships. Use the [CLI and MCP reference](https://richview.ai/docs/tools) for the supported command and tool contracts.
