---
title: Styles and your brand
description: Pick a preset, give your hex colours, and set any of eight settings from xsmall to xlarge.
styles: [scientific]
---

**Give us your hex colors; we pick the rest.** You style a report with three things: a preset, your colours, and a few settings that each take `xsmall`, `small`, `medium`, `large` or `xlarge`. RichView derives every other colour and size, and every combination keeps text readable and charts distinguishable. You never set a token.

## Pick a preset

Name a preset in the report's `styles` list. Without one, a report renders in Sleek.

<!-- registry:begin styles -->
| Style | Default | Menu |
| --- | --- | --- |
| `sleek` | yes | main |
| `air` |  | main |
| `scientific` |  | main |
| `brutalist` |  | main |
| `hand-drawn` |  | main |
| `midnight` |  | more presets |
| `ledger` |  | more presets |
| `meadow` |  | more presets |
| `signal` |  | more presets |
| `chalk` |  | more presets |
<!-- registry:end -->

{{style-comparison}}

Five more presets show what the system makes from a preset, colours and steps: `midnight` (Sleek on navy with gold), `ledger` (Scientific on warm paper, set tight for numbers), `meadow` (Air in forest green with soft corners), `signal` (Brutalist on newsprint with heavy bars) and `chalk` (Hand-drawn on a slate board). Name them like any other preset.

From the working folder containing the `first-report/` tutorial, compare one report in each main preset:

```sh
richview generate first-report/report.rmd --style sleek -o sleek.html
richview generate first-report/report.rmd --style air -o air.html
richview generate first-report/report.rmd --style scientific -o scientific.html
richview generate first-report/report.rmd --style brutalist -o brutalist.html
richview generate first-report/report.rmd --style hand-drawn -o hand-drawn.html
```

Open the files in `first-report/Rendered/`. The observations stay the same; typography, surfaces, palette and chart treatment change. The public tutorial exports also open in [Sleek](https://richview.ai/docs/example/Rendered/sleek), [Air](https://richview.ai/docs/example/Rendered/air), [Scientific](https://richview.ai/docs/example/Rendered/scientific), [Brutalist](https://richview.ai/docs/example/Rendered/brutalist) and [Hand-drawn](https://richview.ai/docs/example/Rendered/hand-drawn).

## Give us your colours

Save this as `report.rstyle.yaml` beside the source:

```yaml style
preset: scientific
colors:
  brand: '#0F62FE'
```

That is a complete brand style. From your brand colour RichView derives the accent, the links and the first chart series, five more series that stay distinct for every kind of colour vision, the heatmap ramps, and text and rules that carry a trace of your hue. Two more colours are optional:

```yaml style
preset: air
colors:
  brand: '#A52A2A'
  secondary: '#167D8D'
  background: '#FFF4DC'
```

| Colour | What it sets | What RichView guarantees |
| --- | --- | --- |
| `brand` | accent, links, the first series | text and links at 4.5:1 or more; marks at 3:1; your exact colour wherever it already reaches its floor |
| `secondary` | the second series | kept exact when it reaches 3:1 and can be told from `brand`; otherwise adjusted, or left out with a note |
| `background` | the page | your exact page, unless no text could reach 7:1 on it or four series 3:1: then it is lightened or darkened only as far as they need, keeping its hue; a dark colour makes a dark report |

Text reaches 7:1 on every surface, secondary text 4.5:1, and the first four series 3:1. A `palette` list of series colours overrides the derived palette and is checked by the palette rule. The [style YAML reference](https://richview.ai/docs/style-yaml) lists every supported setting.

When a colour has to move to reach its floor, `check` says which colour moved and to what. Any CSS colour works; hex is the easiest to copy from a brand guide. Printed pages are always light: a dark page prints as its preset's light counterpart, still in your colours.

Add the file to the report's style list, after the preset or instead of it:

```yaml frontmatter context=examples/first-report
title: Direct visits lead our traffic
description: A first report of one metric, one chart and one callout, ready to edit.
version: 0.0.1
slug: hello-world
styles: [sleek, report.rstyle.yaml]
```

A file's own `preset` takes the place of the preset listed before it. Later files win setting by setting. CLI `--style` replaces the source list for that call; repeat it to name a preset and then a file.

RichView skips a retired setting and reports `style.retired` at its location in the style file. The warning names a supported replacement when one exists. Otherwise the preset supplies the value. Other settings in the same file still apply.

## Set the size of things

Every other setting takes one of five steps. `medium` is the preset's own value, so a setting you leave out changes nothing, and a step means the same change in every preset.

| Setting | What it changes | `xsmall` | `small` | `large` | `xlarge` |
| --- | --- | --- | --- | --- | --- |
| `text-size` | the whole type scale | ×0.8 | ×0.9 | ×1.1 | ×1.2 |
| `spacing` | space between blocks and before sections | ×0.5 | ×0.75 | ×1.33 | ×1.67 |
| `padding` | space inside paragraphs, lists, table cells, callouts, metrics and framed charts | ×0.75 | ×0.875 | ×1.25 | ×1.5 |
| `corners` | images, embeds, code, the readout and bar ends | square | half | rounder | roundest |
| `lines` | header bars, callout and metric rules (hairlines stay at least 1 px) | ×0.5 | ×0.75 | ×1.5 | ×2 |
| `gridlines` | the chart's soft rules: gridlines, and the tracks and empty cells some charts draw | faintest | fainter | stronger | strongest |
| `tint` | how much of your hue the page, rules and text carry | grey | half | ×2 | ×3.5 |
| `motion` | how far and how long blocks and charts move | still | quicker | slower | most |

```yaml style
preset: meadow
colors:
  brand: '#1E7B4F'
text-size: large
spacing: xlarge
corners: xsmall
motion: xsmall
```

The steps are built to combine. The type scale moves as one, so headings stay clearly larger than body text. Space between blocks follows the text size. On phones text moves at most a tenth either way and gaps half as far. Colours are derived after the steps, so `tint` and `gridlines` never take text below its contrast floor, and the faintest gridlines stay visible. Page width and the reading measure stay fixed. Reduced-motion preferences are always honoured.

## Put it on a background

Hand-drawn, Scientific and Sleek ship a background: paper grain, graph paper and a glow. Change it, or turn it off, in one word in the report's frontmatter (`background: dots`, `background: none`), or once for every report in a style file. The [backgrounds guide](https://richview.ai/docs/backgrounds) shows each one.

## Bring your fonts

Use the `fonts` setting to assign display, text, UI and monospace roles. Bundled stacks travel with the report. For a brand font, a role can use a mapping such as `fonts: {text: {family: My Brand, file: media/brand.woff2}}`; supply that WOFF2 file inside the report folder and ensure you can redistribute it. The native exporter embeds the resolved font. A system font name alone cannot make a font portable.

## Review the result

Inspect narrow and desktop layouts, and run `check`: it reports any colour it moved. Sleek prints as Air; [PDF export](https://richview.ai/docs/export) keeps the report readable on paper.
