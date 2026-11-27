---
title: Slides
description: A deck is a .rslide file of named slots on a 24 by 24 grid, one screen a slide, in every style.
styles: [scientific]
---

A deck is a `.rslide` file: slides to present in the browser, one screen each. It is a format of its own, not a report with a switch, and a deck and a report never combine. You choose what each slide holds and where it stands, from a short list of places; the style sets everything else. `slides: true` in a report's frontmatter is retired, and the check tells you to write a `.rslide` file instead.

## Try a live deck

This deck of ten slides is made from real data: NASA's annual global temperature index, 1880 to 2025, the data behind the gallery's report *A warmer world*. It has a cover, a single number, a line chart, a chart beside its note, a table, a section, a chart over two numbers, three numbers side by side and two slides of text. Open it and move with the arrow keys, Space or a swipe. Press `F` in the full deck to fill the screen.

{{report:warmest-years}}

## A small deck

Save this source as `review/deck.rslide`. Its rows are made up.

```rslide
format: rslide/1
title: September business review
description: Monthly recurring revenue reached $630K.
version: 1.0.0
styles: [sleek]
---
title: September business review
subtitle: Where the growth came from
---
metric:
  value: "$630K"
  label: Monthly recurring revenue at the end of September
  delta: 0.26
  delta_label: vs August
source: Made-up monthly rows
---
title: Enterprise added $80K in September; mid-market held at $50K
left:
  - **Enterprise** added $80K, up from $60K in August.
  - **Mid-market** added $50K, the same as in August.
right:
  chart:
    type: bar
    title: Net new MRR by segment
    data:
      inline:
        - { month: Aug, added: 60000, segment: Enterprise }
        - { month: Aug, added: 50000, segment: Mid-market }
        - { month: Sep, added: 80000, segment: Enterprise }
        - { month: Sep, added: 50000, segment: Mid-market }
    encoding: { x: month, y: added, color: segment }
    axes:
      y: { label: Net new MRR, format: number_compact, unit: $ }
cols: 9-15
source: Made-up monthly rows
```

Check it, then render it:

```sh
richview check review/deck.rslide
richview generate review/deck.rslide
```

`check` reports no warnings or errors. Open `review/Rendered/deck.html`: three slides, a cover, one big number and a text beside the chart it explains.

## How a deck is written

The first line is `format: rslide/1`. It is how every tool, a publish server included, knows the text is a deck. Under it, before the first `---`, comes the setup: `title`, `description`, `version`, `slug`, `lang` and `styles`, written as a report's frontmatter is. Every slide then opens with a line of `---`.

A slide is a list of keys at the left edge, in any order. Text is taken as written, with no quoting. Its furniture is the `title` (state the claim, in one line), the `subtitle` (what it is about) and, at the foot, a `source` and a `footer`. The rest is what it holds:

- A **piece** alone: one `chart`, `metric`, `table` or `text`, written under its own key. A number alone is the hero of its slide.
- Or **slots**: `left` and `right`, `top` and `bottom`, `left`, `middle` and `right`, `top`, `middle` and `bottom`, or a `top` over two (`bottom-left` and `bottom-right`), a `bottom` under two, a `left` beside two or a `right` beside two (`top-right` and `bottom-right`, or `top-left` and `bottom-left`). Any other set of slots is the error `deck.layout`, and the message names the nearest layout.

A slot holds Markdown text, or a mapping with exactly one `chart:`, `metric:` or `table:` written the way that block's fence is in a report, indented under the key. A slide with only furniture is a section; the first one is the cover.

## The grid

The stage is 16:9 and cut into 24 columns and 24 rows. A slide with a `title` or `subtitle` has a head in rows 1 to 4, one with a `source` or `footer` has a foot in rows 23 and 24, and the slots share the rest; a slide with neither gives its slots all 24 rows.

Slots share their space equally. `cols` and `rows` cut it differently, as parts of 24 that add up to 24: `cols: 9-15` is the golden split, a text beside the chart it explains, and `cols: 8-8-8` is the thirds. `rows` is a share of the content region, so `rows: 14-10` means the same on every slide. A layout with no cut on an axis takes no setting for it (`deck.span`).

## Each style is a stage

The style you choose sets the stage every slide stands on, each a world of its own: the same slides, with type, colour, framing and arrival changed together. Nothing in the deck's source changes.

- **Sleek** is a stage at night: the title in large type with a blue glow behind it, the single number and the section slides on a deep field of its blue, in white, and charts straight on the dark.
- **Air** is daylight on a Swiss grid: flush left, the single number in light blue figures across the foot of its slide under its label, the section on a pale field.
- **Brutalist** is a wall of posters: condensed capitals to the edges between heavy bars, the single number white on blue over a heavy bar, the section on black.
- **Hand-drawn** is a talk drawn as it is given: lettering at a tilt, the red pencil under each number and on the first series of each chart, rings round its changes and the section taped on as a note.
- **Scientific** is the talk of a good paper: a title page, each section between the title page's two rules, headings over a rule, figures and tables numbered as a paper numbers them.

Choose one in the setup's `styles`, or with `richview generate deck.rslide --style air`, and compare. Every stage reads what you wrote the same way, so write for it:

- Write the `title` as the claim ("MRR rose $240K from July to September") and the `subtitle` as what it is about. On a slide with a head, the head speaks and the chart's own `title` stays for screen readers; on a slide with no head, the chart's title is the headline.
- A big number is a slide of its own, a `metric` alone. Two or three stand in `left`, `middle` and `right`; the first leads. Give each one comparison, named in words (`delta_label: vs August`).
- A chart's first series keeps its colour and the others step back, so put the series the slide is about first.

A slide below the first screen builds as it comes into view, at the style's own pace. With reduced motion, without JavaScript and on paper every slide is at rest. A style file chooses its stage with `components.slides.stage` (`plain`, `keynote`, `gallery`, `poster`, `sketchbook` or `paper`); a style that sets none uses `plain`, the layout alone.

## Keep each slide to one screen

Every render and `richview check` warn `slides.density`, naming the slide, its slot and its line, when a slot holds more text than it reads (about 40 words in a slot under half the stage's width, 80 in a wider one), a title is longer than a title reads, or a table shows more than ten rows. Start a new slide there. `notes` and `reveal` are not read yet: they warn `deck.key` and are ignored.

On a landscape window of 900 px or more a slide is a 16:9 stage, as large as the window allows. Every size follows the stage, so a laptop and a meeting-room screen show the same composition. On a phone each slide is a card, its head first and then its slots in reading order. Nothing is clipped and no text shrinks to fit: a slide that needs more than one screen grows and scrolls.

## Present and print

Readers move with the wheel, a trackpad or a swipe. Space, Page Down and the right and down arrows go on; Shift+Space, Page Up and the left and up arrows go back; Home and End go to the ends. While a chart, table or control has focus it keeps the arrows and Space. `F` fills the screen, and a counter in the corner shows where you are. The address carries the slide, so `deck.html#slide=4` opens the fourth. With reduced motion the slides jump instead of gliding. Without JavaScript the slides still snap and the browser's own keys still scroll them; the counter, `F` and the address do not work.

Printing from the browser gives one slide a page on a 16:9 landscape page, in the style's print colours. `richview generate --to pdf` writes the same pages:

```sh
richview generate review/deck.rslide --to pdf
```

## Share and publish the deck

A deck is one self-contained HTML file, like a report. Open the exported HTML in a browser or use `richview serve` while you edit, share the HTML file for the interactive deck, or send the PDF for a page per slide. A deck publishes through the same path as a report: the setup's `title`, `description` and `version` are what publishing requires, and the hosted page is the deck.
