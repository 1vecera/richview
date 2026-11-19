---
title: Style YAML reference
description: Every style setting and its accepted values, generated from the loader.
styles: [scientific]
---

Start with [styles and your brand](https://richview.ai/docs/styles): a preset, your hex colours and the five-step settings are the whole surface. This page lists every setting from the same grammar that validates style files, then the legacy leaves that still apply.

## How the cascade works

Source `styles` is an ordered list of presets and style files. A file's `preset` takes the place of the preset before it; later settings win, and lists replace lists. A list containing only files starts from Sleek, or from Air when a file brings colours without a preset. CLI `--style` supplies a replacement list for that call; repeat it to choose a preset and then a file. A chart's explicit local style takes priority over inherited chart defaults.

Every setting is safe in any combination: `medium` is the preset's own value, and colours are derived with their contrast floors guaranteed. Unknown leaves report `style.unknown-key`, invalid values report `style.schema`, and attempts to change internal page mechanics report `style.internal`. Legacy leaves apply and report `style.legacy`, naming what replaces them. A bad leaf is skipped while the rest of the file applies. YAML parse failures still need repair.

Colours accept any CSS colour; hex is the easiest to copy from a brand guide. Font stacks name bundled families, or a font setting names a family and a local WOFF2 file. This interface accepts validated data, not arbitrary CSS or JavaScript.

<!-- richview:style-reference -->
