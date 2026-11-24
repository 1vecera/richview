Keep report content in .rmd and appearance in ordered .rstyle.yaml files.

### The presets

The five main presets are:

- **sleek**, the default: light sans-serif type and bright chart lines on a near-black page. It prints as air.
- **air**, its light counterpart: sans-serif type on a white page.
- **scientific**: serif type on a warm off-white page, chart subtitles set as captions.
- **brutalist**: heavy condensed capitals, thick black rules and framed charts.
- **hand-drawn**: marker-style capitals and pencil lines told apart by dashes and point markers. It letters the title, section heads, chart claims, callout titles, table heads and NOTE/SOURCE labels by hand, sets reading text in a serif, and draws its rules, callout bracket and stat underline as seeded pencil strokes.

Five extended presets are style files over them: midnight (sleek on navy, gold brand), ledger (scientific on warm paper, tight), meadow (air in forest green, soft corners), signal (brutalist on newsprint, heavy bars) and chalk (hand-drawn on a slate board). sleek-dark, sleek-light and editorial are retired and report style.renamed.

### Choose and compare styles

- **In the source:** `styles: [scientific]` in the frontmatter. The first entry is the base and later ones override it; a later file wins per leaf and lists replace. A cascade that names only files starts from sleek, or from air when a file brings colours without a preset.
- **For one command:** `--style` on `generate` and `check` replaces the source's `styles` for that call and may repeat: `richview check report.rmd --style scientific`. `validate` takes no `--style`.
- **Several looks of one report:** render the same source once per style, each to its own file in `Rendered/`: `richview generate report.rmd --style scientific -o report-scientific.html`. A style file works the same way, named relative to the report folder whatever the shell's directory: `--style brand.rstyle.yaml -o report-brand.html`. Earlier output files stay until you delete them.

### A style file

Give us your hex colours; we pick the rest. A style file has three parts and an optional background, and nothing else is taught:

- preset: the preset the file starts from (sleek, air, scientific, brutalist, hand-drawn, or the extended midnight, ledger, meadow, signal, chalk). A file's preset takes the place of the preset before it.
- colors: brand (accent, links, first series), secondary (optional second series), background (optional page; a dark colour makes a dark report). Any CSS colour; hex is easiest. RichView derives every other colour: text at 7:1, secondary text and links at 4.5:1, marks and the first four series at 3:1, six series distinct for every kind of colour vision, ramps and status colours. A colour that must move to meet its floor raises the info style.company-adjusted.
- five-step settings, each xsmall, small, medium, large or xlarge, where medium is the preset's own value: text-size (the whole type scale), spacing (between blocks), padding (inside blocks and framed charts), corners (images, code, readout, bar ends), lines (header bars, callout and metric rules), gridlines (the chart's soft rules: gridlines, tracks, empty cells; never invisible), tint (how much of your hue the neutrals carry), motion (xsmall is still).
- background: texture (grain, dots, grid, lines), gradient (soft, glow, from your colours) or picture (a still PNG, JPEG or WebP in the report folder, at most 400 KiB and 2400 px; it stands alone, style.background-conflict otherwise), place (page; margins, where the report sits on a solid sheet; header, where a picture puts the title on a solid panel) and strength (the five steps). Text keeps its contrast over the worst pixel: inks move first, and the background weakens only if it must (info style.background-adjusted). Charts and tables keep solid surfaces, code keeps its own panel, nothing moves, and paper prints the flat page. One word stands for its mapping: background: grain, glow or a picture file. Hand-drawn ships grain, Scientific a small grid and Sleek a glow (so does a report that names no style); Air and Brutalist are flat. background: none gives a flat page, against a preset's default or an earlier file's.
- In the report: the frontmatter takes the same background words, after every style, so the report's word wins: `background: dots`, `background: none`, or the mapping.

Also: fonts (display, text, ui, mono: a bundled family stack, or {family, file} with a .woff2 in the report folder), palette (a builtin palette name or 1 to 8 colours, overriding the derived series), colophon (show, hide), company (name, logo, logo-dark).

Any combination is safe: steps multiply the preset's values, phone text moves at most a tenth, and colours are derived after the steps. Page width and the measure are fixed.

Example:

    preset: scientific
    colors:
      brand: "#8A2432"
      background: "#F7F3EA"
    text-size: small
    corners: xsmall

RichView skips a retired setting and reports style.retired at its location in the style file. The warning names a supported replacement when one exists. Otherwise the preset supplies the value. Other settings in the same file still apply. Invalid values, unknown keys and protected page mechanics have their own findings.

### Motion

Blocks below the first screen fade up once as they come into view, in the preset's own character. The motion setting scales it: xsmall renders everything settled, small is quicker and shorter, large and xlarge slower and further, at most one second. Motion respects prefers-reduced-motion.
