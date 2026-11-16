---
name: richview
description: Write or revise Rich Markdown reports with RichView, using its installed authoring guide, source files, diagnostics and rendered output. Use for RichView report authoring, exports and explicitly requested publishing.
license: MIT
---

Write a report the reader can follow from its claim to the evidence. Keep the report's `.rmd`, local data, media and optional style files together in its own dedicated folder.

Load `richview learn`, or call `richview_learn`, before authoring. Use the installed guide as the source for supported blocks, chart types, fields, styles, commands and tool arguments. Read the relevant section when choosing unfamiliar syntax; do not infer capabilities from a screenshot or an old example. The [tool guide](https://richview.ai/docs/tools.md) and [format reference](https://richview.ai/docs/reference.md) are generated from the code.

Choose a clear question, support assertion titles with the data and cite the source of each material claim. Keep observations, calculations and interpretation distinguishable. For visual judgment, read the `anti-slop` section of `richview learn`, which uses RichView's shared design guidance; apply it to the actual report.

Edit the saved source and inspect the rendered report. Run validation and the geometry/readability check, use their diagnostics to correct the source, and inspect again at the reader's intended size. A clean check does not judge the truth or quality of the story. Preserve the source evidence when changing the presentation.

Export locally when requested. Inspect the exported file and report any required check that did not run. Publish only when the user asks, to the server they name or have already configured; local preparation does not authorize publishing, changing access or deploying a site.
