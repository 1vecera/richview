---
title: Slides
description: One source as slides for a review meeting, with big numbers, charts and short text, one screen each.
styles: [scientific]
---

A new slide format is coming; decks made with `slides: true` will need converting then.

`slides: true` in the frontmatter turns the source into slides. Each top-level heading or `---` starts a slide. The style's stage sets the look.

Add the line to a report's frontmatter to make a deck from the same prose, charts, metrics and tables. Remove it to render the source as a report again.

## Try a live deck

The September business review uses illustrative revenue data. Open the deck and move with the arrow keys, Space or a swipe. Press `F` in the full deck to fill the screen.

{{report:monthly-review}}

## A complete deck

Save this source as `review/report.rmd`. Its rows are made up.

````rmd
---
title: September business review
description: Monthly recurring revenue reached $630K.
date: 2026-10-05
styles: [sleek]
slides: true
---

```metric
value: "$630K"
label: Monthly recurring revenue at the end of September
delta: 0.26
delta_label: vs August
```

# September at a glance

```metric
value: "$630K"
label: MRR
delta: 0.26
delta_label: vs August
```

```metric
value: "$130K"
label: Net new MRR
delta: 0.1818
delta_label: vs August
```

```metric
value: "$420K"
label: Enterprise MRR
delta: 0.2353
delta_label: vs August
```

```chart
type: bar
title: MRR rose $110K in August and $130K in September
stack: true
data:
  inline:
    - { month: Jul, mrr: 280000, segment: Enterprise }
    - { month: Jul, mrr: 110000, segment: Mid-market }
    - { month: Aug, mrr: 340000, segment: Enterprise }
    - { month: Aug, mrr: 160000, segment: Mid-market }
    - { month: Sep, mrr: 420000, segment: Enterprise }
    - { month: Sep, mrr: 210000, segment: Mid-market }
encoding: { x: month, y: mrr, color: segment }
axes:
  y: { label: MRR, format: number_compact, unit: $ }
```

*Source: made-up monthly rows.*

# Where the growth came from

```chart
type: bar
title: Enterprise added $80K in September, mid-market $50K for the second month
data:
  inline:
    - { month: Aug, added: 60000, segment: Enterprise }
    - { month: Aug, added: 50000, segment: Mid-market }
    - { month: Sep, added: 80000, segment: Enterprise }
    - { month: Sep, added: 50000, segment: Mid-market }
encoding: { x: month, y: added, color: segment }
axes:
  y: { label: Net new MRR, format: number_compact, unit: $ }
```

*Source: made-up monthly rows.*

- **Enterprise** added $80K, up from $60K in August.
- **Mid-market** added $50K, the same as in August.

# The quarter in one table

| Segment | July | August | September | Since July |
|---|---:|---:|---:|---:|
| Enterprise | $280K | $340K | $420K | +50% |
| Mid-market | $110K | $160K | $210K | +90.9% |
| Total | $390K | $500K | $630K | +61.5% |

MRR rose $240K from July to September: $140K from enterprise and $100K from mid-market.

# What these rows cannot tell us

- Whether new MRR came from new customers or from expansion.
- How much MRR churned: the rows hold only the net.

```callout
kind: hold
title: Made-up rows
body: Check real billing data before a review like this one leaves the team.
```
````

Check it, then render it:

```sh
richview check review/report.rmd
richview generate review/report.rmd
```

`check` reports no warnings or errors. It may list automatic chart labels omitted at the checked width. Open `review/Rendered/report.html`: six slides, from the title to what the rows cannot tell. The [monthly review](https://richview.ai/docs#a-monthly-review-as-slides) in the gallery is a longer version of this deck.

## How a source becomes slides

The title, description, author and date make the title slide. Each top-level heading starts a slide and is its title. A `---` line starts a slide without a title, for a screen that is only a number or a chart, and a heading with nothing under it is a section slide. Anything before the first heading is a slide of its own, like the single number above.

You choose what is on a slide, never where it sits. A slide sets its `metric` blocks in a row of big numbers, then its charts, tables and images, then its text. Up to four numbers stand in one row and five to eight in two; on a wide screen, two to four numbers over one chart stand in a column beside it instead, so the chart takes the slide's height. One `metric` alone on a slide is set as large as the format goes. One figure takes the room left and two stand side by side. Text beside one figure is a column a third of the slide wide, and text alone is set larger.

A chart on a slide is laid out for its box at the screen's resolution, with lettering that follows the slide's size. It prints the values it adds by itself, as in a report, because on a stage the number is the message: a bar, line or area chart sets them larger than its axes and in their compact form (`$420K`); set `auto_annotations: false` on a chart to leave them out. Its bars take their category's share of the plot, so a few columns fill a wide slide. Its table, downloads and expanded view stay with the report that the same source renders without `slides: true`. A table is set larger the fewer and the shorter its rows, and larger still when it is the slide's only figure, with the slide's text under it.

## Each style is a stage

The style you choose sets the stage the slides stand on, each a world of its own: the same slides, with type, colour, framing and arrival changed together.

- **Sleek** is a stage at night: the title in large type with a blue glow behind it, the single number and the section slides on a deep field of its blue, in white, and charts straight on the dark.
- **Air** is daylight on a Swiss grid: flush left, the single number in light blue figures across the foot of its slide under its label, the section on a pale field.
- **Brutalist** is a wall of posters: condensed capitals to the edges between heavy bars, the single number white on blue over a heavy bar, the section on black.
- **Hand-drawn** is a talk drawn as it is given: lettering at a tilt, the red pencil under each number and on the first series of each chart, rings round its changes and the section taped on as a note.
- **Scientific** is the talk of a good paper: a title page, each section between the title page's two rules, headings over a rule, figures and tables numbered as a paper numbers them.

Every stage reads what you wrote the same way, so write for it:

- A slide whose only figure is one chart shows the chart's title as its headline and its own heading above it as the topic. Write the chart's title as what it shows ("MRR rose $110K in August and $130K in September") and the heading as what the slide is about.
- In a row or a column of numbers the first leads, larger than the rest: put the headline figure first. Each change shows as a chip pointing up or down.
- In a Markdown table, a column of signed changes (`+50%`) shows as chips, the column just before it leads, and a last row that is the sum of the rows above stands under a rule as their total.
- A callout on a slide of text becomes a pull quote beside the points.
- A chart's first series keeps its colour and the others step back, so put the series the slide is about first.

A slide below the first screen builds as it comes into view, at the style's own pace: its title, numbers, figures and text arrive one after another. With reduced motion, without JavaScript and on paper every slide is at rest. A style file chooses its stage with `components.slides.stage` (`plain`, `keynote`, `gallery`, `poster`, `sketchbook` or `paper`); a style that sets none uses `plain`, the layout alone.

## Keep each slide to one screen

Every render and `richview check` warn `slides.density`, naming the slide and its line, when a slide holds more than four numbers beside a figure or more than eight in all, more than two figures, more than 50 words of text beside a figure or a number (80 alone), or a table of more than ten rows. Start a new slide there. `tabs` has no effect in slides, and a source that uses them is warned with `slides.tabs`.

On a landscape window of 900 px or more a slide is a 16:9 stage, as large as the window allows. Every size follows the stage, so a laptop and a meeting-room screen show the same composition. On a phone the slides stand one under another at the style's own sizes. Nothing is clipped and no text shrinks to fit: a slide that needs more than one screen grows and scrolls.

## Present and print

Readers move with the wheel, a trackpad or a swipe. Space, Page Down and the right and down arrows go on; Shift+Space, Page Up and the left and up arrows go back; Home and End go to the ends. While a chart, table or control has focus it keeps the arrows and Space. `F` fills the screen, and a counter in the corner shows where you are. The address carries the slide, so `report.html#slide=4` opens the fourth. With reduced motion the slides jump instead of gliding. Without JavaScript the slides still snap and the browser's own keys still scroll them; the counter, `F` and the address do not work.

Printing from the browser gives one slide a page on a 16:9 landscape page, in the style's print colours. `richview generate --to pdf` writes the same pages:

```sh
richview generate review/report.rmd --to pdf
```

## Open and share the deck

Open the exported HTML in a browser or use `richview serve` while you edit. The VS Code preview and the playground also show the slides. Share the HTML file for the interactive deck, or send the PDF for a page per slide.
