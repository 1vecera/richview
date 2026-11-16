Keep report content in .rmd and appearance in ordered .rstyle.yaml files.

### The builtin styles

The five builtins are:

- **sleek**, the default: light sans-serif type and bright chart lines on a near-black page. It prints as air.
- **air**, its light counterpart: sans-serif type on a white page.
- **scientific**: serif type on a warm off-white page, chart subtitles set as captions.
- **brutalist**: heavy condensed capitals, thick black rules and framed charts.
- **hand-drawn**: marker-style capitals and pencil lines told apart by dashes and point markers. It letters the title, section heads, chart claims, callout titles, table heads and NOTE/SOURCE labels by hand, sets reading text in a serif, and draws its rules, callout bracket and stat underline as seeded pencil strokes (components rules.style: pencil, callout.style: bracket, metric.style: underline; title, heading, table-head and source-label take font: display|text|ui|mono).

sleek-dark, sleek-light and editorial are retired and report style.renamed.

### Choose and compare styles

- **In the source:** `styles: [scientific]` in the frontmatter. The first entry is the base and later ones override it; a later file wins per leaf and lists replace. A cascade that names only files starts from sleek.
- **For one command:** `--style` on `generate` and `check` replaces the source's `styles` for that call and may repeat: `richview check report.rmd --style scientific`. `validate` takes no `--style`.
- **Several looks of one report:** render the same source once per style, each to its own file in `Rendered/`: `richview generate report.rmd --style scientific -o report-scientific.html`. A style file works the same way, named relative to the report folder whatever the shell's directory: `--style brand.rstyle.yaml -o report-brand.html`. Earlier output files stay until you delete them.

### A style file

Settings are top-level keys:

- density (comfortable, loose, airmax)
- block-gap (regular, tight, wide)
- type-size (large, small, bigger)
- accent (any colour; it moves the accent, links and first series together)
- palette (a builtin palette name or 1 to 8 colours)
- motion (on, off)
- colophon (show, hide)
- fonts (display, text, ui, mono: a bundled family stack, or {family, file} with a .woff2 in the report folder)

Beyond the settings a file may set tokens, components and chart leaves; sections are tokens, layout, chart, motion and components. tokens/layout become --rv-* custom properties; size-small to size-display (each with an optional -phone variant) form one type scale. Every leaf passes a closed grammar: a leaf that fails is reported (style.schema for a bad value, style.unknown-key for an unknown leaf, style.internal for page mechanics such as measure and report-max-width) and skipped, and the rest of the file applies. Each leaf is safe or experimental; an experimental leaf raises the info diagnostic style.experimental.

### Motion

Blocks below the first screen fade up once as they come into view (motion.block-entrance: none, fade or fade-up; duration, easing, stagger and distance tune it); motion.enabled: false renders everything settled, and motion.delay and components.callout.left-bar-width report style.retired. Motion respects prefers-reduced-motion.
