---
title: Styles and your brand
description: Change the presentation while keeping the source observations intact.
styles: [scientific]
---

Start with a builtin, then add a small local YAML file if you need your own colours or fonts. The [report gallery](https://richview.ai/docs/) shows the different treatments as complete reports.

<!-- registry:begin styles -->
| Style | Default |
| --- | --- |
| `sleek` | yes |
| `air` |  |
| `scientific` |  |
| `brutalist` |  |
| `hand-drawn` |  |
<!-- registry:end -->

{{style-comparison}}

## Compare the same report

From the working folder containing the `first-report/` tutorial, run:

```sh
richview generate first-report/report.rmd --style sleek -o sleek.html
richview generate first-report/report.rmd --style air -o air.html
richview generate first-report/report.rmd --style scientific -o scientific.html
richview generate first-report/report.rmd --style brutalist -o brutalist.html
richview generate first-report/report.rmd --style hand-drawn -o hand-drawn.html
```

Open the files in `first-report/Rendered/`. The observations stay the same; typography, surfaces, palette and chart treatment change. The public tutorial exports also open in [Sleek](https://richview.ai/docs/example/Rendered/sleek.html), [Air](https://richview.ai/docs/example/Rendered/air.html), [Scientific](https://richview.ai/docs/example/Rendered/scientific.html), [Brutalist](https://richview.ai/docs/example/Rendered/brutalist.html) and [Hand-drawn](https://richview.ai/docs/example/Rendered/hand-drawn.html).

## Add your colours

Save this as `report.rstyle.yaml` beside the source:

```yaml style
accent: 'oklch(0.48 0.16 26)'
palette: ['oklch(0.48 0.16 26)', 'oklch(0.54 0.09 211)']
type-size: large
motion: off
```

The accent setting moves the accent, links and first series together. A palette sets categorical series colours. This example turns motion off for a settled review copy; the builtin's motion remains available when that setting is omitted.

Keep the starter's existing metadata and add the file to its style list:

```yaml frontmatter context=examples/first-report
title: Direct visits lead our traffic
description: A first report of one metric, one chart and one callout, ready to edit.
version: 0.0.1
slug: hello-world
styles: [sleek, report.rstyle.yaml]
```

Later leaves win; arrays replace arrays. Builtins need no YAML file. Without a named builtin, a file-only list starts from Sleek. A chart's explicit `style.palette` takes priority over the inherited palette.

## Try a complete override

CLI `--style` replaces the source style list for that call. Include both entries when you want a builtin plus the file:

```sh
richview generate first-report/report.rmd \
  --style scientific --style report.rstyle.yaml -o branded.html
```

The source remains unchanged. To make the choice persistent, edit its `styles` list instead. Remove a file reference when you remove the file.

## Bring your fonts

Use the `fonts` setting to assign display, text, UI and monospace roles. Bundled stacks travel with the report. For a brand font, a role can use a mapping such as `fonts: {text: {family: My Brand, file: media/brand.woff2}}`; supply that WOFF2 file inside the report folder and ensure you can redistribute it. The native exporter embeds the resolved font. A system font name alone cannot make a font portable.

Use the [style YAML reference](https://richview.ai/docs/style-yaml) for every accepted leaf and value. It separates safe settings from experimental controls. Invalid leaves report diagnostics and are skipped. Raw CSS, HTML and JavaScript are not customization interfaces.

## Review the result

Inspect narrow and desktop layouts. Check that the colours distinguish the series and the text stays legible. Use OKLCH for authored colours; RichView maps output to sRGB. Reduced-motion preferences are always honoured. Sleek prints as Air; [PDF export](https://richview.ai/docs/export) keeps the report readable on paper.
