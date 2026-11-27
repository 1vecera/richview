A deck is a `.rslide` file: slides to present live, full screen in the browser. It is a format of its own, not a report with a switch, and a deck and a report never combine. You choose what each slide holds and where it stands; the style sets everything else.

**The first line of the file is `format: rslide/1`.** It is how every tool, a publish server included, knows the text is a deck. Under it, before the first `---`, comes the setup (`title`, `description`, `version`, `slug`, `lang`, `styles`), written as a report's frontmatter is. Every slide then opens with a line of `---`. A slide is a mapping of keys at the left edge, in any order; text is taken as written, with no quoting.

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

source: Synthetic monthly rows
cols: 9-15
---
metric:
  value: $630K
  label: MRR at the end of September
  delta: 0.26
  delta_label: vs August
```

### What a slide holds

The setup's keys:

@DECKSETUP@

A slide's keys:

@DECKSLIDE@

**A slot holds one piece**: text, or a `chart`, `metric` or `table` mapping with its fields indented under it, written exactly as that block's fence is in a report. Text is Markdown: bold, italics, links, lists, short paragraphs. A slide with one piece and no slots writes the piece under its own key (`chart:`, `metric:`, `table:` or `text:`). There are no layout names: the slots a slide uses are its layout, and any other set of slots is the error `deck.layout`.

@DECKLAYOUTS@

### The grid

The stage is 16:9, cut into 24 columns and 24 rows. A slide with a `title` or `subtitle` has a head in rows 1 to 4, one with a `source` or `footer` has a foot in rows 23 and 24, and the slots share the rest; a slide with neither head nor foot gives the slots all 24 rows. Slots share their space equally. `cols` and `rows` cut it as parts of 24 that add up to 24: `cols: 9-15` is the golden split (a text beside the chart it explains), `cols: 8-8-8` the thirds. `rows` is a share of the content region, so `rows: 9-15` means the same on every slide. A layout with no cut on an axis takes no setting for it (`deck.span`).

### Writing a good deck

- **One idea a slide, and a title that states it.** Write the title as the claim ("MRR rose $240K from July to September") and the subtitle as what it is about. A slide that needs more is two slides.
- **A big number is a slide of its own**, a `metric` alone: the largest type the format sets. Two or three numbers stand in `left`, `middle` and `right`. Give each one comparison, named in words (`delta_label: vs August`).
- **A chart takes the room its slot gives it**, laid out for that box and never scaled. Give a chart a `title` (it is required, and it is the chart's accessible name); on a slide with its own head the slide's title speaks and the chart's stays for screen readers, and on a slide with no head the chart's title is the headline. Put the series the slide is about first: the style steps the others back.
- **Text is short.** A slot under half the stage's width reads up to about 40 words, a wider one 80; `slides.density` names the slide and the slot that holds more. A table shows up to ten rows.
- **A source note belongs to its slide**: `source` stands at the foot's left, `footer` at its right. A slide with neither has no foot, and its slots take the rows.
- **A cover and a section are slides with a `title` and nothing else.** The first one is the cover; the style sets each in its own way.
- **Notes and reveals are not read yet.** `notes` and `reveal` warn (`deck.key`) and are ignored.

### Styles are stages

The style you choose sets the stage every slide stands on, a world of its own: Sleek a stage at night, Air daylight on a grid, Brutalist posters, Hand-drawn a talk drawn as it is given, Scientific the talk of a good paper. Nothing in a deck's source changes with the style; choose one in the setup's `styles` or with `--style` and compare.

### Check, render, present, publish

```sh
richview check deck.rslide
richview generate deck.rslide
richview generate deck.rslide --to pdf
```

`check` returns every finding with its line in the deck; chart, metric and table findings read as they do in a report, with `slides[3].left.chart.encoding.y` as their path. `generate` writes one self-contained HTML deck. Readers move with Space, Page Down and the arrows, `F` fills the screen and `#slide=4` opens the fourth slide. On a window narrower than 900 px each slide is a card, its head first, then its slots in reading order. Printing and `--to pdf` give one slide a page. Publishing takes a deck as it takes a report, and the setup's `title`, `description` and `version` are what it requires.
