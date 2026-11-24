---
title: Backgrounds
description: Put your report on paper grain, graph paper, a soft glow or your own picture, in one word.
styles: [scientific]
---

A background makes a report yours at a glance: paper grain, graph paper, a light from your colour or your own picture behind the page. Three presets ship one, you change it in one word, and text stays as readable as on a flat page.

{{background-comparison}}

## What each preset ships

| Preset | Background | Why |
| --- | --- | --- |
| `hand-drawn` | `grain` | Hand lettering on paper. |
| `scientific` | `grid` at `small` | Graph paper behind the serif text, faint enough to read past. |
| `sleek` | `glow` | A light from the accent in the top corner of the dark page. |
| `air` | none | Its white space is the point. |
| `brutalist` | none | Flat newsprint. |

The extended presets take their base's: `chalk` is Hand-drawn on slate with grain, `ledger` Scientific with its grid, `midnight` Sleek with its glow. A report that names no style renders in Sleek, so it has the glow too.

## Say it in one word

Write `background` in the report's frontmatter. It comes after every style, so the report's word wins:

```yaml frontmatter context=examples/first-report
title: Orders doubled
styles: [scientific]
background: dots
```

| Word | What you get |
| --- | --- |
| `grain` | Paper grain: a fine noise in a shade of the page. |
| `dots` | A dot grid, as on a dotted notebook page. |
| `grid` | Graph paper. |
| `lines` | Ruled lines on the text's own line pitch. |
| `soft` | A gradient from your colour, washing down from the top. |
| `glow` | A gradient from your colour, from the top corner. |
| `hero.jpg` | Your picture: a still PNG, JPEG or WebP in the report folder, at most 400 KiB and 2400 px a side. |
| `none` | A flat page, also against a preset that ships a background. |

The colour of `soft` and `glow` is `colors.brand` from your style file (a glow takes `secondary` when you give one), else the preset's accent. Textures are drawn in a shade of the page itself.

## Turn it off

```yaml frontmatter context=examples/first-report
title: Orders doubled
styles: [hand-drawn]
background: none
```

`background: none` works in the frontmatter and in a style file. The docs pages you are reading are flat by choice: they are the site, not a report, so the docs build turns their preset's background off.

## Place and strength

For more than one word, give a mapping. The same words go in a style file, so a company can set its background once for every report:

```yaml style
background:
  texture: grain       # grain, dots, grid, lines
  gradient: soft       # soft, glow; combines with a texture
  place: margins       # page, margins or header
  strength: small      # xsmall, small, medium, large, xlarge
```

- `place: page`, the default, puts the background behind everything.
- `place: margins` puts it around the report, which sits on a solid sheet; on a phone, where there are no margins, it shows as a band above the report.
- `place: header` puts it behind the title only. A picture goes there by default, and the title then sits on a solid panel.
- `strength` runs from `xsmall` to `xlarge`; `medium` is the designed default.
- A picture stands alone: with a texture or a gradient beside it, RichView keeps the picture, leaves the other out and tells you with `style.background-conflict`.

## What stays readable

A background never costs the reader anything:

- Every ink keeps its contrast over the background's darkest pixel: body text 7:1, secondary text, links and the colours of rises and falls 4.5:1, the accent and chart ticks 3:1.
- Inks you did not set move a little in lightness to keep that contrast. If they cannot, the background weakens instead, and `richview check` says so with the info `style.background-adjusted`.
- A picture under text is washed just enough to hold over a black or a white pixel.
- Charts and tables keep their solid surfaces, and code its own panel, so they read as cards on the background.
- Nothing moves. Paper and PDF print the flat page, and high-contrast and forced-colour modes drop the background.

## A complete example

A report folder with a picture behind the title and the company's colours:

```text
race/
  report.rmd
  report.rstyle.yaml
  media/san-francisco-at-dusk.webp
```

The style file:

```yaml style
preset: air
colors:
  brand: "#0F5C45"
```

The report's frontmatter:

```yaml frontmatter context=examples/openai-vs-anthropic
title: The most expensive race
styles: [report.rstyle.yaml]
background:
  picture: media/san-francisco-at-dusk.webp
  place: header
```

`richview check race` reports a picture outside the folder, missing, larger than 400 KiB or 2400 px, animated or not a PNG, JPEG or WebP, and the report renders without it. Every word is in the [style YAML reference](https://richview.ai/docs/style-yaml).
