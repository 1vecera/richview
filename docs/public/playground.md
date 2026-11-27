---
title: Use the browser playground
description: Edit Rich Markdown, compare styles and export a draft without a native install.
styles: [scientific]
---

[Open the playground](https://richview.ai/playground) to try RichView in your browser. Start with its example or open a chart example’s **Open in playground** link. The source editor and rendered preview sit together.

## Edit the source, data and style

Write ordinary Markdown around tagged blocks. Paste drafts with inline rows. **Open in playground** loads a supplied example with its data and style files. To render a folder on your own disk, run `richview generate` with the path to its `.rmd` file. A browser draft cannot run SQL or MetricFlow queries.

Choose a builtin style to compare the same observations. Keep the original values, units and source note as you change the presentation. Read the diagnostics before exporting; a preview can remain available while a block needs repair.

## Keep a copy

Export your source and generated HTML through the playground controls. The draft belongs to this browser session; download it before closing or moving to another machine. A share link, when offered, carries the draft in its URL fragment, so treat it as shared content and avoid confidential data.

For a durable folder workflow, use [the native command](https://richview.ai/docs/first-report). Publishing to a report server is a separate operation. To change a report that is already on the hosted app, use the [online editor](https://richview.ai/docs/online-editor); it edits the same way and saves a new version.
