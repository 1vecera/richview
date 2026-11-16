---
title: Style YAML reference
description: The accepted style leaves and their value grammar, generated from the loader.
styles: [scientific]
---

Start with [styles and your brand](https://richview.ai/docs/styles) for a small working override. This page lists the author-settable leaves from the same grammar that validates style files. Internal layout mechanics are excluded.

## How the cascade works

Source `styles` is an ordered list. Later leaves win and lists replace lists. A list containing only local files starts from the default builtin. CLI `--style` supplies a replacement list for that call; repeat it to choose a builtin and then a file. A chart's explicit local style takes priority over inherited chart defaults.

Safe leaves are the normal customization surface. Experimental leaves apply but emit `style.experimental`; inspect the result at the widths you intend to share. Unknown leaves report `style.unknown-key`, invalid values report `style.schema`, and attempts to change internal page mechanics report `style.internal`. A bad leaf is skipped while the rest of the file applies. YAML parse failures still need repair.

Use OKLCH for colour authoring. Font stacks name bundled families, or a font setting names a family and a local WOFF2 file. This interface accepts validated data, not arbitrary CSS or JavaScript.

<!-- richview:style-reference -->
